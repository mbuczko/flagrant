use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use sqlx::{Decode, Encode, Sqlite, Type, encode::IsNull, sqlite::SqliteValueRef};
use std::{fmt, str::FromStr};
use strum_macros::{Display, EnumIter, EnumString};
use thiserror::Error;
use utoipa::ToSchema;

extern crate regex;

pub mod payload;

// max variant size is 1kb (1024 bytes)
const MAX_VARIANT_SIZE: usize = 1024;

#[derive(Debug, Error)]
pub enum ParseTypeError {
    #[error("'{0}' is an unknown value type")]
    Type(String),

    #[error("Value incorrectly encoded")]
    Encoding,

    #[error("Value exceeds max size of 1024 bytes")]
    SizeExceeded,
}

#[derive(Debug, Default, Serialize, Deserialize, sqlx::FromRow, Validate, ToSchema)]
pub struct Project {
    #[sqlx(rename = "project_id")]
    pub id: i32,
    #[validate(pattern = r"^[A-Za-z][A-Za-z0-9_]+$")]
    #[validate(max_length = 255)]
    pub name: String,
}

#[derive(Debug, Default, Serialize, Deserialize, sqlx::FromRow, Validate, ToSchema)]
pub struct Environment {
    #[sqlx(rename = "environment_id")]
    pub id: i32,
    pub project_id: i32,
    #[validate(pattern = r"^[A-Za-z][A-Za-z0-9_]+$")]
    #[validate(max_length = 255)]
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow, Validate, ToSchema)]
pub struct Feature {
    #[sqlx(rename = "feature_id")]
    pub id: i32,
    pub project_id: i32,
    #[validate(pattern = r"^[A-Za-z][A-Za-z0-9_]+$")]
    #[validate(max_length = 255)]
    pub name: String,
    #[validate(max_length = 2048)]
    pub description: String,
    pub variants: Vec<Variant>,
    pub tags: TagList,
    pub is_enabled: bool,
    pub is_archived: bool,
    pub is_srv: bool,
    /// Optimistic-concurrency counter, bumped by one on every commit that touches this
    /// feature (property change or delete). Global per feature, not per-environment.
    pub version: i32,
    /// `Some` marks this feature as a progressive rollout - a single alternative
    /// variant's weight ramping up over a defined schedule of steps. Feature-level
    /// (shared across environments) by design; only the live progression state (see
    /// `flagrant::models::rollout`) and the weight it drives are per-environment.
    /// `#[sqlx(skip)]`: like `variants`, this is never populated by the derive - only
    /// by the manual `row_to_feature` mapping in `flagrant::models::feature`.
    #[sqlx(skip)]
    pub rollout: Option<RolloutConfig>,
}

/// One step of a progressive rollout's schedule: hold the alternative variant at
/// `weight` for `hold_for_secs` before advancing to the next step. `hold_for_secs` is
/// `None` only on the last (terminal) step - nothing follows a terminal step to hold
/// for. See [`RolloutConfig::validate_steps`] for the invariants this must satisfy.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RolloutStep {
    pub weight: u8,
    pub hold_for_secs: Option<u32>,
}

/// A feature's progressive-rollout definition, stored as raw JSON on the `features` row
/// (see [`Feature::rollout`]) so its mere presence tells you whether the feature is
/// "progressive" without a separate flag.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RolloutConfig {
    /// Minimum number of organically-distributed identities required before the
    /// schedule may advance past its first step. Checked once, before the very first
    /// timed advance - never re-checked on later steps, so a rollout won't stall
    /// mid-schedule if traffic later drops.
    #[serde(default = "RolloutConfig::default_min_sample_size")]
    pub min_sample_size: u32,
    pub steps: Vec<RolloutStep>,
}

impl RolloutConfig {
    pub fn default_min_sample_size() -> u32 {
        100
    }

    /// Cross-step invariants that a single field's shape can't express on its own:
    /// at least one step, weights non-decreasing, and exactly one step - the last -
    /// with no hold duration. Called explicitly from both the CLI and `feature::patch`
    /// so the rule can't be bypassed by going through one path but not the other -
    /// mirrors [`Comparator::validate_value`].
    pub fn validate_steps(&self) -> Result<(), &'static str> {
        let n = self.steps.len();
        if n == 0 {
            return Err("A progressive rollout requires at least one step.");
        }
        for step in &self.steps {
            if step.weight > 100 {
                return Err("Rollout step weights must be between 0 and 100.");
            }
        }
        for (i, step) in self.steps.iter().enumerate() {
            let is_last = i + 1 == n;
            if is_last && step.hold_for_secs.is_some() {
                return Err(
                    "The last rollout step is terminal and must not carry a hold duration.",
                );
            }
            if !is_last && step.hold_for_secs.is_none() {
                return Err("Every rollout step but the last must carry a hold duration.");
            }
        }
        if self.steps.windows(2).any(|w| w[1].weight < w[0].weight) {
            return Err("Rollout step weights must be non-decreasing.");
        }
        Ok(())
    }
}

/// Live view of a feature's progressive rollout in one environment - the schedule plus
/// where it currently stands. Returned by the read-only rollout-status endpoint.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RolloutStatus {
    pub config: RolloutConfig,
    pub current_step: i32,
    pub last_change_at: NaiveDateTime,
    pub distributed_identities: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct Variant {
    #[sqlx(rename = "variant_id")]
    pub id: i32,
    pub value: VariantValue,
    pub weight: u8,
    pub accumulator: i32,
    pub environment_id: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum VariantValue {
    Text(String),
    Json(String),
    Toml(String),
}

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct Identity {
    pub id: i32,
    pub value: String,
    #[serde(skip)]
    pub environment_id: i32,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow, Validate, ToSchema)]
pub struct Trait {
    #[sqlx(rename = "trait_id")]
    pub id: i32,
    // Restricted to a safe charset (no quotes/commas/brackets) so names can be embedded,
    // unescaped, into the JSON blobs `identity::list` builds to filter by trait via
    // SQLite's json_each() - see flagrant::models::traits for the full rationale.
    #[validate(pattern = r"^[A-Za-z0-9._@-]+$")]
    #[validate(max_length = 255)]
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate, ToSchema)]
pub enum TraitValue {
    // Same charset restriction as `Trait::name`, and for the same reason: string trait
    // values are embedded unescaped into the same hand-built JSON filter blobs.
    Str(
        #[validate(pattern = r"^[A-Za-z0-9._@-]*$")]
        #[validate(max_length = 1024)]
        String,
    ),
    Int(i32),
    Float(f32),
    Bool(bool),
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct IdentityTrait {
    pub trait_id: i32,
    pub name: String,
    pub value: Option<TraitValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IdentityWithTraits {
    pub id: i32,
    pub value: String,
    pub traits: Vec<IdentityTrait>,
}

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct IdentityVariant {
    pub variant_id: Option<i32>,
    pub feature_id: i32,
    pub identity_id: Option<i32>,
    pub migrated_id: Option<i32>,
    pub segment_id: Option<i32>,
    pub segment_dirty: bool,
    pub feature_name: String,
    pub feature_value: Option<VariantValue>,
    pub pinned_at: Option<NaiveDateTime>,
    pub is_srv: bool,
    pub is_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Subject {
    /// Match against the identity value string (e.g. email, user ID).
    Identity,
    /// Match against a named identity trait. The `String` is the trait name.
    Trait(String),
    /// Match against the environment name.
    Environment,
}

/// Single source of truth for `Subject`'s string form - used by the `Encode` impl below
/// (DB storage) and by the CLI (rule display, menu labels), so both always agree.
impl fmt::Display for Subject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Identity => write!(f, "identity"),
            Self::Trait(name) => write!(f, "trait:{name}"),
            Self::Environment => write!(f, "environment"),
        }
    }
}

/// `strum(serialize = ...)` below is the single source of truth for this enum's string
/// form - `Display`/`FromStr` (used by the `Encode`/`Decode` impls below, and by the CLI's
/// `Comparator::iter()`-driven parsing/display) all derive from it, so DB storage, the API
/// wire format, and the CLI always agree and a new variant can't be missed in any of them.
#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema, Display, EnumIter, EnumString,
)]
#[serde(rename_all = "snake_case")]
pub enum Comparator {
    #[strum(serialize = "exactly_matches")]
    ExactlyMatches,
    #[strum(serialize = "does_not_match")]
    DoesNotMatch,
    #[strum(serialize = "contains")]
    Contains,
    #[strum(serialize = "does_not_contain")]
    DoesNotContain,
    #[strum(serialize = "greater_than")]
    GreaterThan,
    #[strum(serialize = "greater_equal_than")]
    GreaterEqualThan,
    #[strum(serialize = "lower_than")]
    LowerThan,
    #[strum(serialize = "lower_equal_than")]
    LowerEqualThan,
    /// Value must be a JSON array string, e.g. `["a","b"]`.
    #[strum(serialize = "in")]
    In,
    /// Value must be a JSON array string.
    #[strum(serialize = "not_in")]
    NotIn,
}

impl Comparator {
    /// Validates that `value` is well-formed for this comparator - currently only `In`/
    /// `NotIn` have a constraint, requiring a JSON array. Shared by the CLI and the API/DB
    /// layer so the check can't be bypassed by going through one path but not the other.
    pub fn validate_value(&self, value: &str) -> Result<(), &'static str> {
        if matches!(self, Comparator::In | Comparator::NotIn)
            && serde_json::from_str::<Vec<serde_json::Value>>(value).is_err()
        {
            return Err(
                "Value must be a valid JSON array for the 'in'/'not_in' comparator, e.g. [\"a\",\"b\"].",
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct SegmentRule {
    #[sqlx(rename = "rule_id")]
    pub id: i32,
    #[sqlx(rename = "driver")]
    pub subject: Subject,
    pub comparator: Comparator,
    /// For `In`/`NotIn` comparators this is a JSON array string; otherwise a plain value.
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum GroupConnector {
    And,
    AndNot,
}

impl sqlx::Type<Sqlite> for Subject {
    fn type_info() -> <Sqlite as sqlx::Database>::TypeInfo {
        <String as Type<Sqlite>>::type_info()
    }
}
impl Encode<'_, Sqlite> for Subject {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as sqlx::Database>::ArgumentBuffer<'_>,
    ) -> Result<IsNull, sqlx::error::BoxDynError> {
        Encode::<Sqlite>::encode(self.to_string(), buf)
    }
}
impl<'r> Decode<'r, Sqlite> for Subject {
    fn decode(value: SqliteValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <&str as sqlx::Decode<Sqlite>>::decode(value)?;
        match s {
            "identity" => Ok(Self::Identity),
            "environment" => Ok(Self::Environment),
            _ if s.starts_with("trait:") => Ok(Self::Trait(s[6..].to_string())),
            _ => Err(format!("Unknown segment subject: {s}").into()),
        }
    }
}

impl sqlx::Type<Sqlite> for Comparator {
    fn type_info() -> <Sqlite as sqlx::Database>::TypeInfo {
        <String as Type<Sqlite>>::type_info()
    }
}
impl Encode<'_, Sqlite> for Comparator {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as sqlx::Database>::ArgumentBuffer<'_>,
    ) -> Result<IsNull, sqlx::error::BoxDynError> {
        Encode::<Sqlite>::encode(self.to_string(), buf)
    }
}
impl<'r> Decode<'r, Sqlite> for Comparator {
    fn decode(value: SqliteValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <&str as sqlx::Decode<Sqlite>>::decode(value)?;
        s.parse()
            .map_err(|_| format!("Unknown comparator: {s}").into())
    }
}

impl sqlx::Type<Sqlite> for GroupConnector {
    fn type_info() -> <Sqlite as sqlx::Database>::TypeInfo {
        <String as Type<Sqlite>>::type_info()
    }
}
impl Encode<'_, Sqlite> for GroupConnector {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as sqlx::Database>::ArgumentBuffer<'_>,
    ) -> Result<IsNull, sqlx::error::BoxDynError> {
        let s = match self {
            Self::And => "and",
            Self::AndNot => "and_not",
        };
        Encode::<Sqlite>::encode(s, buf)
    }
}
impl<'r> Decode<'r, Sqlite> for GroupConnector {
    fn decode(value: SqliteValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <&str as sqlx::Decode<Sqlite>>::decode(value)?;
        match s {
            "and" => Ok(Self::And),
            "and_not" => Ok(Self::AndNot),
            _ => Err(format!("Unknown group connector: {s}").into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct SegmentGroup {
    #[sqlx(rename = "group_id")]
    pub id: i32,
    /// Stable auto-generated label (e.g. "group-1"). Never reassigned after deletion.
    pub label: String,
    pub description: Option<String>,
    /// `None` for the first (head) group; `Some` for all subsequent groups.
    pub connector: Option<GroupConnector>,
    pub rules: Vec<SegmentRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, Validate, ToSchema)]
pub struct Segment {
    #[sqlx(rename = "segment_id")]
    pub id: i32,
    pub project_id: i32,
    #[validate(pattern = r"^[A-Za-z][A-Za-z0-9_]+$")]
    #[validate(max_length = 255)]
    pub name: String,
    #[validate(max_length = 2048)]
    pub description: Option<String>,
    /// Optimistic-concurrency counter, bumped by one on every commit that touches this
    /// segment (property/group/rule change or delete - groups and rules have no version
    /// of their own, so any op targeting them via `SegmentPatchOp` bumps this instead).
    /// Echo the value you last fetched back on a commit's `SegmentCommitPart.version` to
    /// have the server reject it if the segment changed elsewhere in the meantime.
    pub version: i32,
    /// Groups ordered by position; first group has `connector = None`.
    pub groups: Vec<SegmentGroup>,
}

/// A single variant's snapshotted state - value/weight as of right after a commit.
/// `accumulator` is deliberately excluded: it's live distributor state, rebalanced
/// fresh whenever weights are (re)applied, not configuration worth restoring.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SnapshotVariant {
    pub id: i32,
    pub value: VariantValue,
    pub weight: u8,
    pub is_control: bool,
}

/// A segment group as captured inside a snapshot's segment override - enough to
/// recreate the group (and its rules) if the owning segment was later deleted. Vec
/// order (not a stored position field) carries the intended group order - mirrors
/// `Segment::groups`, which already omits `position` for the same reason.
#[derive(Debug, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct SnapshotSegmentGroup {
    pub label: String,
    pub connector: Option<GroupConnector>,
    pub description: Option<String>,
    pub rules: Vec<SegmentRule>,
}

/// A segment's full definition plus its weight override for one feature, as of right
/// after a commit. Carries the full definition (not just `segment_id`) so a restore can
/// recreate the segment if it was deleted in the meantime - see the "Restoring segment
/// overrides" rationale in the snapshots design.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SnapshotSegmentOverride {
    pub segment_id: i32,
    pub segment_name: String,
    pub segment_description: Option<String>,
    pub groups: Vec<SnapshotSegmentGroup>,
    pub weights: Vec<payload::SegmentVariantWeight>,
}

/// A single pinned identity override as captured inside a snapshot. Organic
/// (non-pinned) assignments are never part of snapshot state - only deliberate,
/// bounded `OVERRIDE add` pins are.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SnapshotIdentityOverride {
    pub identity_id: i32,
    pub identity_value: String,
    pub variant_id: i32,
}

/// The full materialized state of a feature captured by a snapshot - self-contained,
/// so restoring never depends on replaying any other snapshot or patch.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SnapshotState {
    pub name: String,
    pub description: String,
    pub is_enabled: bool,
    pub is_srv: bool,
    pub is_archived: bool,
    pub tags: Vec<String>,
    pub variants: Vec<SnapshotVariant>,
    pub segment_overrides: Vec<SnapshotSegmentOverride>,
    pub identity_overrides: Vec<SnapshotIdentityOverride>,
    /// The progressive rollout's config and step index at capture time, if the feature
    /// had one active - `None`/`None` otherwise. Captured together (not just the step)
    /// so a restore is self-contained: a step index only means something relative to
    /// the exact schedule it was recorded against, which may have since been replaced by
    /// a different one (a rule change resets progression to step 0 going forward, but
    /// that offers no protection for an *older* snapshot predating the change).
    pub rollout_config: Option<RolloutConfig>,
    pub rollout_step: Option<i32>,
}

/// A numbered, commented, point-in-time snapshot of a feature's state within one
/// environment, recorded automatically by every commit that affects it.
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct Snapshot {
    #[sqlx(rename = "snapshot_id")]
    pub id: i32,
    pub feature_id: i32,
    pub environment_id: i32,
    pub version: i32,
    pub comment: Option<String>,
    /// Raw JSON text - parse via [`Snapshot::parsed_state`]. Kept untyped at the row
    /// level so a future addition to `SnapshotState` doesn't require a migration.
    pub state: String,
    pub created_at: NaiveDateTime,
}

impl Snapshot {
    pub fn parsed_state(&self) -> serde_json::Result<SnapshotState> {
        serde_json::from_str(&self.state)
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SnapshotDiff {
    pub target: Snapshot,
    pub current: SnapshotState,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", content = "value", rename_all = "lowercase")]
pub enum FeatureOverride {
    Identity {
        value: String,
        variant_id: i32,
    },
    Segment {
        name: String,
        weights: Vec<payload::SegmentVariantWeight>,
    },
}

/// A single variant's weight within a segment override, carrying enough to display it
/// (value, whether it's the control variant) without a separate variant lookup.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OverriddenVariant {
    pub variant_id: i32,
    pub value: VariantValue,
    pub is_control: bool,
    pub weight: u8,
}

/// A feature that a segment overrides, with the full weight breakdown (including the
/// control variant's auto-balanced remainder) for a given environment.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SegmentFeatureOverride {
    pub feature_id: i32,
    pub feature_name: String,
    pub weights: Vec<OverriddenVariant>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
#[schema(value_type = Vec<Tag>)]
pub struct TagList(pub Vec<Tag>);

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct Tag {
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct FeatureResponse {
    pub feature_id: i32,
    pub name: String,
    pub value: VariantValue,
    pub is_enabled: bool,
    pub is_srv: bool,
}

impl Feature {
    pub fn get_default_variant(&self) -> &Variant {
        self.variants
            .iter()
            .find(|v| v.is_control())
            .expect("Feature has no default variant!")
    }
    pub fn get_default_value(&self) -> &VariantValue {
        &self.get_default_variant().value
    }
    pub fn with_variants(mut self, variants: Vec<Variant>) -> Self {
        self.variants = variants;
        self
    }
}

impl Variant {
    pub fn build(id: i32, value: VariantValue, weight: u8) -> Variant {
        Variant {
            id,
            value,
            weight,
            accumulator: weight as i32,
            environment_id: None,
        }
    }
    pub fn build_default(environment: &Environment, id: i32, value: VariantValue) -> Variant {
        Variant {
            id,
            value,
            weight: 100,
            accumulator: 100,
            environment_id: Some(environment.id),
        }
    }
    pub fn is_control(&self) -> bool {
        self.environment_id.is_some()
    }
}

impl sqlx::Type<sqlx::Sqlite> for TagList {
    fn type_info() -> <sqlx::Sqlite as sqlx::Database>::TypeInfo {
        <String as Type<Sqlite>>::type_info()
    }
}

impl sqlx::Type<sqlx::Sqlite> for VariantValue {
    fn type_info() -> <sqlx::Sqlite as sqlx::Database>::TypeInfo {
        <String as Type<Sqlite>>::type_info()
    }
}

// It's not really used. Tags are are normalized and stored in separate table
// but since entire Feature is Serialize, TagList needs to be Serialize too.
impl Encode<'_, Sqlite> for TagList {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as sqlx::Database>::ArgumentBuffer<'_>,
    ) -> Result<IsNull, sqlx::error::BoxDynError> {
        if self.0.is_empty() {
            Ok(IsNull::Yes)
        } else {
            Encode::<Sqlite>::encode(
                self.0
                    .iter()
                    .map(|tag| tag.name.as_str())
                    .collect::<Vec<_>>()
                    .join(","),
                buf,
            )
        }
    }
}

impl<'r> Decode<'r, Sqlite> for TagList {
    fn decode(value: SqliteValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let value = <&str as sqlx::Decode<Sqlite>>::decode(value)?;
        if !value.is_empty() {
            let tags: Vec<Tag> = value
                .split(',')
                .filter_map(|tag| {
                    let name = tag.trim().to_string();
                    if name.is_empty() {
                        return None;
                    }
                    Some(Tag { name })
                })
                .collect();
            return Ok(TagList(tags));
        }
        Ok(TagList(Vec::new()))
    }
}

impl fmt::Display for TagList {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_fmt(core::format_args!(
            "{}",
            self.0
                .iter()
                .map(|tag| tag.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }
}

impl Encode<'_, Sqlite> for VariantValue {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as sqlx::Database>::ArgumentBuffer<'_>,
    ) -> Result<IsNull, sqlx::error::BoxDynError> {
        Encode::<Sqlite>::encode(self.to_string(), buf)
    }
}

impl<'r> Decode<'r, Sqlite> for VariantValue {
    fn decode(value: SqliteValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let value = <&str as sqlx::Decode<Sqlite>>::decode(value)?;
        Self::from_str(value).map_err(Into::into)
    }
}

impl Default for VariantValue {
    fn default() -> Self {
        Self::Text(String::default())
    }
}

impl fmt::Display for VariantValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let (typ, val) = self.decompose();
        write!(f, "{typ}::{}", val.trim())
    }
}
impl VariantValue {
    fn new(typ: &str, value: &str) -> Result<Self, ParseTypeError> {
        let val = value.to_owned();
        match typ {
            "json" => Ok(Self::Json(val)),
            "toml" => Ok(Self::Toml(val)),
            "text" => Ok(Self::Text(val)),
            _ => Err(ParseTypeError::Type(typ.to_owned())),
        }
    }
    pub fn decompose(&self) -> (&str, &str) {
        match self {
            Self::Json(v) => ("json", v),
            Self::Toml(v) => ("toml", v),
            Self::Text(v) => ("text", v),
        }
    }
    /// The value without its `type::` prefix (may be multi-line).
    pub fn bare(&self) -> &str {
        self.decompose().1
    }
    /// The value's first line only, without its `type::` prefix - the compact,
    /// single-line form used almost everywhere a value is displayed inline.
    pub fn bare_first_line(&self) -> &str {
        let bare = self.bare();
        bare.lines().next().unwrap_or(bare)
    }
    pub fn build(value: &str) -> Self {
        let val = value.trim();
        Self::from_str(val).unwrap_or_else(|_| match val.chars().next() {
            Some('{') => Self::Json(val.to_owned()),
            Some('[') => Self::Toml(val.to_owned()),
            _ => Self::Text(val.to_owned()),
        })
    }
    pub fn clone_with(&self, value: &str) -> Self {
        let (typ, _) = self.decompose();
        Self::new(typ, value).unwrap()
    }
}

impl FromStr for VariantValue {
    type Err = ParseTypeError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if let Some((typ, val)) = value.split_once("::") {
            return if val.len() > MAX_VARIANT_SIZE {
                Err(ParseTypeError::SizeExceeded)
            } else {
                Self::new(typ, val)
            };
        }
        Err(ParseTypeError::Encoding)
    }
}

impl fmt::Display for TraitValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Str(v) => write!(f, "str::{v}"),
            Self::Int(v) => write!(f, "int::{v}"),
            Self::Float(v) => write!(f, "float::{v}"),
            Self::Bool(v) => write!(f, "bool::{v}"),
        }
    }
}

impl TraitValue {
    /// Infers the type from the raw string and returns the appropriate variant.
    /// Detection order: bool → i32 → f32 → Str.
    pub fn build(value: &str) -> Self {
        if let Ok(b) = value.parse::<bool>() {
            return Self::Bool(b);
        }
        if let Ok(i) = value.parse::<i32>() {
            return Self::Int(i);
        }
        if let Ok(f) = value.parse::<f32>() {
            return Self::Float(f);
        }
        Self::Str(value.to_owned())
    }
}

impl FromStr for TraitValue {
    type Err = ParseTypeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (typ, val) = s.split_once("::").ok_or(ParseTypeError::Encoding)?;
        match typ {
            "str" => Ok(Self::Str(val.to_owned())),
            "int" => val
                .parse::<i32>()
                .map(Self::Int)
                .map_err(|_| ParseTypeError::Encoding),
            "float" => val
                .parse::<f32>()
                .map(Self::Float)
                .map_err(|_| ParseTypeError::Encoding),
            "bool" => val
                .parse::<bool>()
                .map(Self::Bool)
                .map_err(|_| ParseTypeError::Encoding),
            _ => Err(ParseTypeError::Type(typ.to_owned())),
        }
    }
}

impl sqlx::Type<Sqlite> for TraitValue {
    fn type_info() -> <Sqlite as sqlx::Database>::TypeInfo {
        <String as Type<Sqlite>>::type_info()
    }
}

impl Encode<'_, Sqlite> for TraitValue {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as sqlx::Database>::ArgumentBuffer<'_>,
    ) -> Result<IsNull, sqlx::error::BoxDynError> {
        Encode::<Sqlite>::encode(self.to_string(), buf)
    }
}

impl<'r> Decode<'r, Sqlite> for TraitValue {
    fn decode(value: SqliteValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let s = <&str as sqlx::Decode<Sqlite>>::decode(value)?;
        Self::from_str(s).map_err(Into::into)
    }
}
