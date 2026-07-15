pub mod environments;
pub mod features;
pub mod identities;
pub mod projects;
pub mod segments;
pub mod tags;
pub mod traits;
pub mod variants;

use smallvec::{SmallVec, smallvec};

type IncludedExcludedTuple<'a> = (
    Option<SmallVec<[&'a str; 3]>>, // Included
    Option<SmallVec<[&'a str; 3]>>, // Excluded
);

/// Parses pattern parameter: wraps non-empty string with SQL wildcards.
pub fn parse_pattern(pattern: Option<String>, prefix: Option<String>) -> Option<String> {
    match (
        pattern.filter(|s| !s.is_empty()),
        prefix.filter(|s| !s.is_empty()),
    ) {
        (Some(p), _) => Some(format!("%{p}%")),
        (_, Some(p)) => Some(format!("{p}%")),
        _ => None,
    }
}

/// Parses a comma-separated parameter (e.g. tags or trait names) into included and
/// excluded lists. Entries prefixed with '-' are excluded, others are included.
pub fn parse_included_excluded<'a>(values: Option<&'a String>) -> IncludedExcludedTuple<'a> {
    values
        .map(|values| {
            let (mut included, mut excluded) = (smallvec![], smallvec![]);

            for value in values.split(',') {
                if let Some(value) = value.strip_prefix('-')
                    && !value.is_empty()
                {
                    excluded.push(value);
                } else if !value.is_empty() {
                    included.push(value);
                }
            }

            (
                if included.is_empty() {
                    None
                } else {
                    Some(included)
                },
                if excluded.is_empty() {
                    None
                } else {
                    Some(excluded)
                },
            )
        })
        .unwrap_or((None, None))
}
