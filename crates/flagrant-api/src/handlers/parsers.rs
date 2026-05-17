use smallvec::{SmallVec, smallvec};

type TagsTuple<'a> = (
    Option<SmallVec<[&'a str; 3]>>, // Tags included
    Option<SmallVec<[&'a str; 3]>>, // Tags excluded
);

/// Parses pattern parameter: wraps non-empty string with SQL wildcards.
pub fn parse_pattern(pattern: Option<String>, prefix: Option<String>) -> Option<String> {
    match (pattern, prefix) {
        (Some(p), _) => Some(format!("%{p}%")),
        (_, Some(p)) => Some(format!("{p}%")),
        _ => None,
    }
}

/// Parses status parameter: converts non-empty string to bool (true if "active").
pub fn parse_status(status: Option<String>) -> Option<bool> {
    status.filter(|s| !s.is_empty()).map(|s| s == "active")
}

/// Parses state parameter: converts non-empty string to bool (true if "on").
pub fn parse_state(state: Option<String>) -> Option<bool> {
    state.filter(|s| !s.is_empty()).map(|s| s == "on")
}

/// Parses tags parameter into included and excluded tag lists.
/// Tags prefixed with '-' are excluded, others are included.
pub fn parse_tags<'a>(tags: Option<&'a String>) -> TagsTuple<'a> {
    tags.map(|tags| {
        let (mut included, mut excluded) = (smallvec![], smallvec![]);

        for tag in tags.split(',') {
            if let Some(tag) = tag.strip_prefix('-')
                && !tag.is_empty()
            {
                excluded.push(tag);
            } else if !tag.is_empty() {
                included.push(tag);
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
