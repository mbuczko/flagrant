pub mod commit;
pub mod environment;
pub mod feature;
pub mod identity;
pub mod project;
pub mod rule;
pub mod segment;
pub mod snapshot;
pub mod tag;
pub mod traits;
pub mod variant;

/// Encodes a set of names as a JSON array string (e.g. `["a","b"]`) suitable for
/// SQLite's `json_each()`, used to pass a variable-length filter list as a single
/// bound parameter. Goes through real JSON serialization (not manual string-wrapping)
/// since a name containing a `"` or `,` would otherwise corrupt the array's structure
/// once embedded in the query. Returns `None` when `names` is `None`.
pub(crate) fn into_json_string(names: Option<smallvec::SmallVec<[&str; 3]>>) -> Option<String> {
    names.map(|vt| serde_json::to_string(vt.as_slice()).expect("&[&str] serialization cannot fail"))
}
