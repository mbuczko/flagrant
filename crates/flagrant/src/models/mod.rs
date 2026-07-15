pub mod environment;
pub mod feature;
pub mod identity;
pub mod project;
pub mod rule;
pub mod segment;
pub mod tag;
pub mod traits;
pub mod variant;

fn surround_string(s: &str, open_ch: char, close_ch: char) -> String {
    let mut buf = String::with_capacity(s.len() + 2);
    buf.push(open_ch);
    buf.push_str(s);
    buf.push(close_ch);
    buf
}

/// Encodes a set of names as a JSON array string (e.g. `["a","b"]`) suitable for
/// SQLite's `json_each()`, used to pass a variable-length filter list as a single
/// bound parameter. Returns `None` when `names` is `None`.
pub(crate) fn into_json_string(names: Option<smallvec::SmallVec<[&str; 3]>>) -> Option<String> {
    names.map(|vt| {
        let quoted: Vec<String> = vt.iter().map(|t| surround_string(t, '"', '"')).collect();
        surround_string(&quoted.join(","), '[', ']')
    })
}
