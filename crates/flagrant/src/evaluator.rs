//! Rule-based segment evaluator.
//!
//! Resolves which (if any) segment overriding a given feature matches a request context
//! (environment + identity), and returns that segment's id for the caller to pass into
//! `distributor::distribute` (which scopes the weighted pick, and its accumulator state,
//! to that segment).

use std::{borrow::Cow, cmp::Ordering};

use flagrant_types::{
    Comparator, Environment, GroupConnector, IdentityTrait, Project, Segment, SegmentDriver,
    SegmentGroup, SegmentRule, TraitValue,
};
use sqlx::SqliteConnection;

use crate::models::segment;

/// A borrowed view of just the identity data the evaluator needs (value + traits).
///
/// Deliberately not `flagrant_types::IdentityWithTraits`: that type owns its `value: String`,
/// which would force callers to clone it on every evaluation. This runs on the
/// feature-resolution hot path (once per undistributed feature per request), so callers
/// build this from data they already hold, borrowed for the duration of one evaluation.
pub struct IdentityContext<'a> {
    pub value: &'a str,
    pub traits: &'a [IdentityTrait],
}

/// The value a rule's driver resolves to, for comparison against `SegmentRule.value`.
///
/// Distinct from `flagrant_types::TraitValue` on purpose: `TraitValue` is the domain type
/// for an identity trait's stored value. `Identity`/`Environment` drivers don't resolve to
/// a trait at all - they read the identity's own value / the environment's name - so
/// wrapping them in `TraitValue` would misrepresent them as trait data. `ActualValue` is the
/// evaluator's own "comparable value" shape; a `Trait(name)` driver converts the identity's
/// `TraitValue` into one.
///
/// Borrows rather than owns (so `Str(&'a str)`, not `Str(String)`) so resolving a driver
/// never needs to clone the identity's value, the environment's name, or a trait's string -
/// everything it points at already lives in `Environment`/`IdentityContext`/`SegmentRule`
/// for at least as long as one rule evaluation.
#[derive(Debug, Clone, Copy, PartialEq)]
enum ActualValue<'a> {
    Str(&'a str),
    Int(i32),
    Float(f32),
    Bool(bool),
}

impl<'a> From<&'a TraitValue> for ActualValue<'a> {
    fn from(value: &'a TraitValue) -> Self {
        match value {
            TraitValue::Str(s) => ActualValue::Str(s),
            TraitValue::Int(i) => ActualValue::Int(*i),
            TraitValue::Float(f) => ActualValue::Float(*f),
            TraitValue::Bool(b) => ActualValue::Bool(*b),
        }
    }
}

/// Evaluates every segment overriding `feature_id` in `environment`, in priority order
/// (oldest segment first), and returns the id of the first segment whose rules match
/// `identity`. Returns `None` if no segment overrides this feature, or none of them match.
pub async fn evaluate(
    conn: &mut SqliteConnection,
    environment: &Environment,
    identity: &IdentityContext<'_>,
    feature_id: i32,
) -> anyhow::Result<Option<i32>> {
    let candidates = segment::list_overrides_for_feature(conn, environment.id, feature_id).await?;
    let project = Project {
        id: environment.project_id,
        ..Default::default()
    };

    for (segment_id, _name, _weights) in candidates {
        let seg = segment::get_by_id(conn, &project, segment_id).await?;
        if segment_matches(&seg, environment, identity) {
            return Ok(Some(segment_id));
        }
    }
    Ok(None)
}

/// Folds groups left-to-right: the first group is the base predicate; each subsequent
/// group ANDs or AND-NOTs the running result per its `connector`. A segment with no
/// groups never matches.
fn segment_matches(
    segment: &Segment,
    environment: &Environment,
    identity: &IdentityContext<'_>,
) -> bool {
    let mut acc: Option<bool> = None;
    for group in &segment.groups {
        let group_match = group_matches(group, environment, identity);
        acc = Some(match &acc {
            None => group_match,
            Some(prev) => match group.connector {
                Some(GroupConnector::AndNot) => *prev && !group_match,
                _ => *prev && group_match,
            },
        });
    }
    acc.unwrap_or(false)
}

/// A group matches if ANY of its rules match (rules within a group are OR-ed). A group
/// with no rules never matches (`any()` over an empty iterator is `false`).
fn group_matches(
    group: &SegmentGroup,
    environment: &Environment,
    identity: &IdentityContext<'_>,
) -> bool {
    group
        .rules
        .iter()
        .any(|rule| rule_matches(rule, environment, identity))
}

/// Resolves `rule.driver` to an actual value, then dispatches to `comparator_matches`.
/// Fail-closed: a `Trait(name)` driver whose trait is absent from `identity` (or present
/// with `value: None`) never matches, regardless of comparator polarity.
fn rule_matches(
    rule: &SegmentRule,
    environment: &Environment,
    identity: &IdentityContext<'_>,
) -> bool {
    let Some(actual) = resolve_actual(&rule.driver, environment, identity) else {
        return false;
    };
    comparator_matches(&rule.comparator, &actual, &rule.value)
}

/// Resolves the driver to the concrete value from the request context. `Identity` and
/// `Environment` are plain contextual strings, not trait data - only `Trait(name)` involves
/// an actual `TraitValue`, converted here into the evaluator's own `ActualValue`.
fn resolve_actual<'a>(
    driver: &SegmentDriver,
    environment: &'a Environment,
    identity: &IdentityContext<'a>,
) -> Option<ActualValue<'a>> {
    match driver {
        SegmentDriver::Identity => Some(ActualValue::Str(identity.value)),
        SegmentDriver::Environment => Some(ActualValue::Str(&environment.name)),
        SegmentDriver::Trait(name) => identity
            .traits
            .iter()
            .find(|t| &t.name == name)
            .and_then(|t| t.value.as_ref())
            .map(ActualValue::from),
    }
}

fn comparator_matches(comparator: &Comparator, actual: &ActualValue, rule_value: &str) -> bool {
    match comparator {
        Comparator::ExactlyMatches => {
            parse_as(actual, rule_value).is_some_and(|parsed| *actual == parsed)
        }
        Comparator::DoesNotMatch => {
            !parse_as(actual, rule_value).is_some_and(|parsed| *actual == parsed)
        }
        Comparator::Contains => as_plain_string(actual).contains(rule_value),
        Comparator::DoesNotContain => !as_plain_string(actual).contains(rule_value),
        Comparator::GreaterThan => {
            matches!(numeric_cmp(actual, rule_value), Some(Ordering::Greater))
        }
        Comparator::GreaterEqualThan => matches!(
            numeric_cmp(actual, rule_value),
            Some(Ordering::Greater | Ordering::Equal)
        ),
        Comparator::LowerThan => matches!(numeric_cmp(actual, rule_value), Some(Ordering::Less)),
        Comparator::LowerEqualThan => matches!(
            numeric_cmp(actual, rule_value),
            Some(Ordering::Less | Ordering::Equal)
        ),
        Comparator::In => in_set(actual, rule_value),
        Comparator::NotIn => !in_set(actual, rule_value),
    }
}

fn numeric_cmp(actual: &ActualValue, rule_value: &str) -> Option<Ordering> {
    let parsed = parse_as(actual, rule_value)?;
    match (actual, &parsed) {
        (ActualValue::Int(x), ActualValue::Int(y)) => Some(x.cmp(y)),
        (ActualValue::Float(x), ActualValue::Float(y)) => x.partial_cmp(y),
        _ => None,
    }
}

/// Parses `raw` into the same `ActualValue` variant as `actual` (type-directed parse).
/// `None` on parse failure. Borrows `raw` for the `Str` case rather than allocating.
fn parse_as<'a>(actual: &ActualValue, raw: &'a str) -> Option<ActualValue<'a>> {
    match actual {
        ActualValue::Str(_) => Some(ActualValue::Str(raw)),
        ActualValue::Int(_) => raw.parse::<i32>().ok().map(ActualValue::Int),
        ActualValue::Float(_) => raw.parse::<f32>().ok().map(ActualValue::Float),
        ActualValue::Bool(_) => raw.parse::<bool>().ok().map(ActualValue::Bool),
    }
}

/// Plain string form for `Contains`/`DoesNotContain` - not `TraitValue`'s `Display` (which
/// prefixes with `"int::"`/`"bool::"`/etc). E.g. `Int(42)` -> `"42"`. Borrows for the `Str`
/// case (the common one); only `Int`/`Float`/`Bool` need to allocate a formatted string.
fn as_plain_string<'a>(actual: &ActualValue<'a>) -> Cow<'a, str> {
    match actual {
        ActualValue::Str(s) => Cow::Borrowed(*s),
        ActualValue::Int(i) => Cow::Owned(i.to_string()),
        ActualValue::Float(f) => Cow::Owned(f.to_string()),
        ActualValue::Bool(b) => Cow::Owned(b.to_string()),
    }
}

/// `In`/`NotIn` membership: parses `rule_value` as a JSON array, checks whether any element
/// type-appropriately equals `actual`. Malformed JSON is assumed not to occur (validated at
/// write time); if it does, this simply returns `false` rather than panicking.
fn in_set(actual: &ActualValue, rule_value: &str) -> bool {
    let Ok(items) = serde_json::from_str::<Vec<serde_json::Value>>(rule_value) else {
        return false;
    };
    items.iter().any(|item| json_equals(actual, item))
}

fn json_equals(actual: &ActualValue, item: &serde_json::Value) -> bool {
    match actual {
        ActualValue::Str(s) => item.as_str() == Some(*s),
        ActualValue::Int(i) => item.as_i64() == Some(*i as i64),
        ActualValue::Float(f) => item.as_f64() == Some(*f as f64),
        ActualValue::Bool(b) => item.as_bool() == Some(*b),
    }
}

#[cfg(test)]
mod tests {
    use flagrant_types::{IdentityTrait, SegmentDriver};

    use super::*;

    fn env(name: &str) -> Environment {
        Environment {
            id: 1,
            project_id: 1,
            name: name.to_string(),
            description: None,
        }
    }

    /// Owns the value/traits so tests can build one, then borrow an `IdentityContext` from
    /// it as many times as needed via `.ctx()`.
    struct TestIdentity {
        value: String,
        traits: Vec<IdentityTrait>,
    }

    impl TestIdentity {
        fn ctx(&self) -> IdentityContext<'_> {
            IdentityContext {
                value: &self.value,
                traits: &self.traits,
            }
        }
    }

    fn identity(value: &str, traits: Vec<(&str, Option<TraitValue>)>) -> TestIdentity {
        TestIdentity {
            value: value.to_string(),
            traits: traits
                .into_iter()
                .enumerate()
                .map(|(i, (name, value))| IdentityTrait {
                    trait_id: i as i32,
                    name: name.to_string(),
                    value,
                })
                .collect(),
        }
    }

    fn rule(driver: SegmentDriver, comparator: Comparator, value: &str) -> SegmentRule {
        SegmentRule {
            id: 1,
            driver,
            comparator,
            value: value.to_string(),
        }
    }

    fn group(connector: Option<GroupConnector>, rules: Vec<SegmentRule>) -> SegmentGroup {
        SegmentGroup {
            id: 1,
            label: "group-1".to_string(),
            description: None,
            connector,
            rules,
        }
    }

    fn segment(groups: Vec<SegmentGroup>) -> Segment {
        Segment {
            id: 1,
            project_id: 1,
            name: "seg".to_string(),
            description: None,
            groups,
        }
    }

    // -- comparator_matches --------------------------------------------------------------

    #[test]
    fn exactly_matches_compares_within_the_actual_values_type() {
        assert!(comparator_matches(
            &Comparator::ExactlyMatches,
            &ActualValue::Int(42),
            "42"
        ));
        assert!(!comparator_matches(
            &Comparator::ExactlyMatches,
            &ActualValue::Int(42),
            "43"
        ));
        // Unparseable as the actual's type => never matches.
        assert!(!comparator_matches(
            &Comparator::ExactlyMatches,
            &ActualValue::Int(42),
            "not-a-number"
        ));
    }

    #[test]
    fn does_not_match_is_negation() {
        assert!(comparator_matches(
            &Comparator::DoesNotMatch,
            &ActualValue::Str("a"),
            "b"
        ));
        assert!(!comparator_matches(
            &Comparator::DoesNotMatch,
            &ActualValue::Str("a"),
            "a"
        ));
    }

    #[test]
    fn contains_uses_plain_string_form_not_typed_display() {
        assert!(comparator_matches(
            &Comparator::Contains,
            &ActualValue::Int(1234),
            "23"
        ));
        assert!(!comparator_matches(
            &Comparator::Contains,
            &ActualValue::Int(1234),
            "int::"
        ));
    }

    #[test]
    fn ordering_comparators_never_match_on_str_or_bool_actual_values() {
        assert!(!comparator_matches(
            &Comparator::GreaterThan,
            &ActualValue::Str("z"),
            "a"
        ));
        assert!(!comparator_matches(
            &Comparator::GreaterThan,
            &ActualValue::Bool(true),
            "false"
        ));
    }

    #[test]
    fn ordering_comparators_compare_numerically() {
        assert!(comparator_matches(
            &Comparator::GreaterThan,
            &ActualValue::Int(10),
            "5"
        ));
        assert!(comparator_matches(
            &Comparator::GreaterEqualThan,
            &ActualValue::Int(10),
            "10"
        ));
        assert!(comparator_matches(
            &Comparator::LowerThan,
            &ActualValue::Float(1.5),
            "2.0"
        ));
        assert!(comparator_matches(
            &Comparator::LowerEqualThan,
            &ActualValue::Float(2.0),
            "2.0"
        ));
    }

    #[test]
    fn in_and_not_in_check_json_array_membership() {
        assert!(comparator_matches(
            &Comparator::In,
            &ActualValue::Str("b"),
            r#"["a","b","c"]"#
        ));
        assert!(!comparator_matches(
            &Comparator::In,
            &ActualValue::Str("z"),
            r#"["a","b","c"]"#
        ));
        assert!(comparator_matches(
            &Comparator::NotIn,
            &ActualValue::Int(5),
            "[1,2,3]"
        ));
        assert!(!comparator_matches(
            &Comparator::NotIn,
            &ActualValue::Int(2),
            "[1,2,3]"
        ));
    }

    // -- rule_matches / resolve_actual ----------------------------------------------------

    #[test]
    fn identity_driver_matches_against_identity_value() {
        let id = identity("user-42", vec![]);
        let r = rule(
            SegmentDriver::Identity,
            Comparator::ExactlyMatches,
            "user-42",
        );
        assert!(rule_matches(&r, &env("prod"), &id.ctx()));
    }

    #[test]
    fn environment_driver_matches_against_environment_name() {
        let id = identity("user-42", vec![]);
        let r = rule(
            SegmentDriver::Environment,
            Comparator::ExactlyMatches,
            "prod",
        );
        assert!(rule_matches(&r, &env("prod"), &id.ctx()));
        assert!(!rule_matches(&r, &env("staging"), &id.ctx()));
    }

    #[test]
    fn trait_driver_matches_named_trait_value() {
        let id = identity(
            "user-42",
            vec![("plan", Some(TraitValue::Str("premium".into())))],
        );
        let r = rule(
            SegmentDriver::Trait("plan".into()),
            Comparator::ExactlyMatches,
            "premium",
        );
        assert!(rule_matches(&r, &env("prod"), &id.ctx()));
    }

    #[test]
    fn trait_driver_never_matches_when_trait_absent_regardless_of_polarity() {
        let id = identity("user-42", vec![]);
        let matches_rule = rule(
            SegmentDriver::Trait("plan".into()),
            Comparator::ExactlyMatches,
            "premium",
        );
        let does_not_match_rule = rule(
            SegmentDriver::Trait("plan".into()),
            Comparator::DoesNotMatch,
            "premium",
        );
        assert!(!rule_matches(&matches_rule, &env("prod"), &id.ctx()));
        assert!(!rule_matches(&does_not_match_rule, &env("prod"), &id.ctx()));
    }

    #[test]
    fn trait_driver_never_matches_when_trait_present_with_no_value() {
        let id = identity("user-42", vec![("plan", None)]);
        let does_not_match_rule = rule(
            SegmentDriver::Trait("plan".into()),
            Comparator::DoesNotMatch,
            "premium",
        );
        assert!(!rule_matches(&does_not_match_rule, &env("prod"), &id.ctx()));
    }

    // -- group_matches (OR over rules) -----------------------------------------------------

    #[test]
    fn group_matches_if_any_rule_matches() {
        let id = identity("user-42", vec![]);
        let g = group(
            None,
            vec![
                rule(SegmentDriver::Identity, Comparator::ExactlyMatches, "nope"),
                rule(
                    SegmentDriver::Identity,
                    Comparator::ExactlyMatches,
                    "user-42",
                ),
            ],
        );
        assert!(group_matches(&g, &env("prod"), &id.ctx()));
    }

    #[test]
    fn empty_group_never_matches() {
        let id = identity("user-42", vec![]);
        let g = group(None, vec![]);
        assert!(!group_matches(&g, &env("prod"), &id.ctx()));
    }

    // -- segment_matches (AND / AND-NOT fold over groups) ----------------------------------

    #[test]
    fn segment_with_no_groups_never_matches() {
        let id = identity("user-42", vec![]);
        assert!(!segment_matches(&segment(vec![]), &env("prod"), &id.ctx()));
    }

    #[test]
    fn and_connector_requires_both_groups_to_match() {
        let id = identity("user-42", vec![]);
        let head = group(
            None,
            vec![rule(
                SegmentDriver::Identity,
                Comparator::ExactlyMatches,
                "user-42",
            )],
        );
        let matching_tail = group(
            Some(GroupConnector::And),
            vec![rule(
                SegmentDriver::Environment,
                Comparator::ExactlyMatches,
                "prod",
            )],
        );
        let non_matching_tail = group(
            Some(GroupConnector::And),
            vec![rule(
                SegmentDriver::Environment,
                Comparator::ExactlyMatches,
                "staging",
            )],
        );

        assert!(segment_matches(
            &segment(vec![head.clone(), matching_tail]),
            &env("prod"),
            &id.ctx()
        ));
        assert!(!segment_matches(
            &segment(vec![head, non_matching_tail]),
            &env("prod"),
            &id.ctx()
        ));
    }

    #[test]
    fn and_not_connector_requires_tail_group_to_not_match() {
        let id = identity("user-42", vec![]);
        let head = group(
            None,
            vec![rule(
                SegmentDriver::Identity,
                Comparator::ExactlyMatches,
                "user-42",
            )],
        );
        let non_matching_tail = group(
            Some(GroupConnector::AndNot),
            vec![rule(
                SegmentDriver::Environment,
                Comparator::ExactlyMatches,
                "staging",
            )],
        );
        let matching_tail = group(
            Some(GroupConnector::AndNot),
            vec![rule(
                SegmentDriver::Environment,
                Comparator::ExactlyMatches,
                "prod",
            )],
        );

        assert!(segment_matches(
            &segment(vec![head.clone(), non_matching_tail]),
            &env("prod"),
            &id.ctx()
        ));
        assert!(!segment_matches(
            &segment(vec![head, matching_tail]),
            &env("prod"),
            &id.ctx()
        ));
    }

    #[test]
    fn empty_non_head_group_never_contributes_a_match() {
        let id = identity("user-42", vec![]);
        let head = group(
            None,
            vec![rule(
                SegmentDriver::Identity,
                Comparator::ExactlyMatches,
                "user-42",
            )],
        );
        let empty_tail = group(Some(GroupConnector::And), vec![]);
        assert!(!segment_matches(
            &segment(vec![head, empty_tail]),
            &env("prod"),
            &id.ctx()
        ));
    }
}
