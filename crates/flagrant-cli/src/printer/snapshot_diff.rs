use std::collections::HashSet;

use colored::Colorize;
use flagrant_types::{
    SnapshotIdentityOverride, SnapshotSegmentOverride, SnapshotState, SnapshotVariant,
};
use similar::{ChangeTag, TextDiff};

use super::tabular::{rollout::format_duration, segment::format_connector};

/// One line of a git-diff-style comparison: unchanged, only present on the current
/// (live) side, only present on the target (snapshot) side, or present on both but with
/// different text - word-highlighted rather than shown as a plain removed+added pair, so
/// a long line doesn't have to be read in full to spot what actually changed.
enum Line {
    Same(String),
    Removed(String),
    Added(String),
    Changed { old: String, new: String },
}

impl Line {
    fn changed(&self) -> bool {
        !matches!(self, Line::Same(_))
    }

    fn print(&self) {
        match self {
            Line::Same(s) => println!("    {s}"),
            Line::Removed(s) => println!("  {}", format!("- {s}").red()),
            Line::Added(s) => println!("  {}", format!("+ {s}").green()),
            Line::Changed { old, new } => {
                let (removed, added) = word_diff_sides(old, new);
                println!("  {}{removed}", "- ".red());
                println!("  {}{added}", "+ ".green());
            }
        }
    }
}

/// Word-level highlight of a changed text field: words common to both sides are plain
/// red/green (matching the surrounding removed/added convention), the words that
/// actually differ are bold on a black background - so a long line (a description, a
/// segment's rule summary) stands out where it changed instead of forcing a full
/// re-read of the whole line.
fn word_diff_sides(old: &str, new: &str) -> (String, String) {
    let diff = TextDiff::from_words(old, new);
    let mut removed = String::new();
    let mut added = String::new();

    for change in diff.iter_all_changes() {
        let text = change.as_str().unwrap_or_default();
        match change.tag() {
            ChangeTag::Equal => {
                removed.push_str(&text.red().to_string());
                added.push_str(&text.green().to_string());
            }
            ChangeTag::Delete => removed.push_str(&text.red().bold().on_black().to_string()),
            ChangeTag::Insert => added.push_str(&text.green().bold().on_black().to_string()),
        }
    }

    (removed, added)
}

const LABEL_WIDTH: usize = 12;

fn field(label: &str, value: &str) -> String {
    format!("{label:<LABEL_WIDTH$}{value}")
}

fn scalar_lines(out: &mut Vec<Line>, label: &str, current: &str, target: &str) {
    if current == target {
        out.push(Line::Same(field(label, current)));
    } else {
        out.push(Line::Removed(field(label, current)));
        out.push(Line::Added(field(label, target)));
    }
}

/// Same as `scalar_lines`, but for free-text fields worth word-diffing rather than
/// showing as a plain removed+added pair - e.g. `description`, which can run long.
fn scalar_line_worddiff(out: &mut Vec<Line>, label: &str, current: &str, target: &str) {
    if current == target {
        out.push(Line::Same(field(label, current)));
    } else {
        out.push(Line::Changed {
            old: field(label, current),
            new: field(label, target),
        });
    }
}

fn status_str(state: &SnapshotState) -> &'static str {
    if state.is_archived {
        "ARCHIVED"
    } else if state.is_enabled {
        "ON"
    } else {
        "OFF"
    }
}

fn variant_line(v: &SnapshotVariant) -> String {
    let marker = if v.is_control { "★" } else { " " };
    format!("{:>3}% {marker}{}", v.weight, v.value)
}

/// Matches `current`'s variants against `target`'s by id, falling back to value for a
/// variant whose id doesn't appear on the other side - the same fallback `restore`'s
/// reconciliation uses, but read-only: this only classifies each variant as
/// unchanged/changed/removed/added for display, it never resolves or applies anything.
fn variant_lines(current: &[SnapshotVariant], target: &[SnapshotVariant]) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut used_target: HashSet<i32> = HashSet::new();

    for cv in current {
        let tv = target
            .iter()
            .find(|tv| tv.id == cv.id && !used_target.contains(&tv.id))
            .or_else(|| {
                target
                    .iter()
                    .find(|tv| tv.value == cv.value && !used_target.contains(&tv.id))
            });

        match tv {
            Some(tv) => {
                used_target.insert(tv.id);
                if cv.value != tv.value || cv.weight != tv.weight {
                    lines.push(Line::Changed {
                        old: variant_line(cv),
                        new: variant_line(tv),
                    });
                } else {
                    lines.push(Line::Same(variant_line(cv)));
                }
            }
            None => lines.push(Line::Removed(variant_line(cv))),
        }
    }

    for tv in target {
        if !used_target.contains(&tv.id) {
            lines.push(Line::Added(variant_line(tv)));
        }
    }

    lines
}

fn segment_override_line(state: &SnapshotState, ov: &SnapshotSegmentOverride) -> String {
    let weights = ov
        .weights
        .iter()
        .map(|w| {
            let value = state
                .variants
                .iter()
                .find(|v| v.id == w.variant_id)
                .map(|v| v.value.bare_first_line().to_string())
                .unwrap_or_else(|| format!("#{}", w.variant_id));
            format!("{value} → {}%", w.weight)
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("{} {weights}", ov.segment_name)
}

/// Summarizes a segment override's rule groups as captured in the snapshot (or live) -
/// each group's connector and `subject comparator value` rules, OR-ed within a group,
/// combined across groups by their connector (mirrors `SEGMENT show`'s own AND/AND-NOT
/// symbols) - so a rule/group edit on the overriding segment shows up even when the
/// weights it drives haven't changed.
fn segment_rules_summary(ov: &SnapshotSegmentOverride) -> String {
    ov.groups
        .iter()
        .map(|g| {
            let connector = g
                .connector
                .as_ref()
                .map(|c| format!("{} ", format_connector(c)))
                .unwrap_or_default();
            let rules = g
                .rules
                .iter()
                .map(|r| format!("{} {} {}", r.subject, r.comparator, r.value))
                .collect::<Vec<_>>()
                .join(" OR ");
            format!("{connector}[{rules}]")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A segment override's definition line - its description and rule groups as captured -
/// shown only when it differs between current and target, since it's not part of the
/// weights line every override already prints.
fn segment_definition_line(ov: &SnapshotSegmentOverride) -> String {
    let desc = ov.segment_description.as_deref().unwrap_or("");
    let rules = segment_rules_summary(ov);
    if desc.is_empty() {
        format!("{}  {rules}", ov.segment_name)
    } else {
        format!("{}  \"{desc}\"  {rules}", ov.segment_name)
    }
}

/// Matches `current`'s segment overrides against `target`'s by `segment_id` - a segment
/// override only exists at all while a segment actually overrides the feature, so unlike
/// variants there's no value-based fallback to consider. Compares both the weights the
/// override drives and the overriding segment's own definition (name, description, rule
/// groups) as captured at each end - a segment can change shape (a rename, description
/// edit, or rule/group change) without its weights ever moving.
fn segment_override_lines(current: &SnapshotState, target: &SnapshotState) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut used_target: HashSet<i32> = HashSet::new();

    for cov in &current.segment_overrides {
        match target
            .segment_overrides
            .iter()
            .find(|tov| tov.segment_id == cov.segment_id)
        {
            Some(tov) => {
                used_target.insert(tov.segment_id);
                let mut cw: Vec<(i32, u8)> = cov
                    .weights
                    .iter()
                    .map(|w| (w.variant_id, w.weight))
                    .collect();
                let mut tw: Vec<(i32, u8)> = tov
                    .weights
                    .iter()
                    .map(|w| (w.variant_id, w.weight))
                    .collect();
                cw.sort();
                tw.sort();

                if cw != tw || cov.segment_name != tov.segment_name {
                    lines.push(Line::Removed(segment_override_line(current, cov)));
                    lines.push(Line::Added(segment_override_line(target, tov)));
                } else {
                    lines.push(Line::Same(segment_override_line(current, cov)));
                }

                if cov.segment_description != tov.segment_description || cov.groups != tov.groups {
                    lines.push(Line::Changed {
                        old: segment_definition_line(cov),
                        new: segment_definition_line(tov),
                    });
                }
            }
            None => lines.push(Line::Removed(segment_override_line(current, cov))),
        }
    }

    for tov in &target.segment_overrides {
        if !used_target.contains(&tov.segment_id) {
            lines.push(Line::Added(segment_override_line(target, tov)));
        }
    }

    lines
}

fn identity_override_line(state: &SnapshotState, ov: &SnapshotIdentityOverride) -> String {
    let value = state
        .variants
        .iter()
        .find(|v| v.id == ov.variant_id)
        .map(|v| v.value.bare_first_line().to_string())
        .unwrap_or_else(|| format!("#{}", ov.variant_id));
    format!("{} → {value}", ov.identity_value)
}

/// Matches `current`'s pinned identity overrides against `target`'s by `identity_id`.
fn identity_override_lines(current: &SnapshotState, target: &SnapshotState) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut used_target: HashSet<i32> = HashSet::new();

    for cov in &current.identity_overrides {
        match target
            .identity_overrides
            .iter()
            .find(|tov| tov.identity_id == cov.identity_id)
        {
            Some(tov) => {
                used_target.insert(tov.identity_id);
                if cov.variant_id != tov.variant_id {
                    lines.push(Line::Removed(identity_override_line(current, cov)));
                    lines.push(Line::Added(identity_override_line(target, tov)));
                } else {
                    lines.push(Line::Same(identity_override_line(current, cov)));
                }
            }
            None => lines.push(Line::Removed(identity_override_line(current, cov))),
        }
    }

    for tov in &target.identity_overrides {
        if !used_target.contains(&tov.identity_id) {
            lines.push(Line::Added(identity_override_line(target, tov)));
        }
    }

    lines
}

fn rollout_summary(state: &SnapshotState) -> Option<String> {
    state.rollout_config.as_ref().map(|cfg| {
        let schedule = cfg
            .steps
            .iter()
            .map(|s| match s.hold_for_secs {
                Some(secs) => format!("{}% for {}", s.weight, format_duration(secs)),
                None => format!("{}%", s.weight),
            })
            .collect::<Vec<_>>()
            .join(" → ");
        match state.rollout_step {
            Some(step) => format!("{schedule} (step {step})"),
            None => schedule,
        }
    })
}

fn print_section(title: &str, lines: Vec<Line>) {
    if lines.is_empty() {
        return;
    }
    println!("  {}", title.dimmed());
    for line in &lines {
        line.print();
    }
}

/// Prints a git-diff-style comparison between a feature's current live state and a
/// snapshot version's captured state - what a `SNAPSHOT restore <version>` to that
/// version would change. Removed (current-only / old value) lines are red, added
/// (target-only / new value) lines are green, unchanged lines are plain.
pub fn print(current: &SnapshotState, target: &SnapshotState, version: i32) {
    println!(
        "\n{}\n",
        format!("SNAPSHOT diff  current ↔ v{version}").bold()
    );

    let mut top = Vec::new();
    scalar_lines(&mut top, "name", &current.name, &target.name);
    scalar_lines(&mut top, "status", status_str(current), status_str(target));
    scalar_lines(
        &mut top,
        "server-side",
        if current.is_srv { "ON" } else { "OFF" },
        if target.is_srv { "ON" } else { "OFF" },
    );
    if !current.description.is_empty() || !target.description.is_empty() {
        scalar_line_worddiff(
            &mut top,
            "description",
            &current.description,
            &target.description,
        );
    }

    let variants = variant_lines(&current.variants, &target.variants);
    let tags = tag_lines(current, target);
    let segments = segment_override_lines(current, target);
    let identities = identity_override_lines(current, target);
    let rollout = rollout_lines(current, target);

    let any_changed = top.iter().any(Line::changed)
        || variants.iter().any(Line::changed)
        || tags.iter().any(Line::changed)
        || segments.iter().any(Line::changed)
        || identities.iter().any(Line::changed)
        || rollout.iter().any(Line::changed);

    if !any_changed {
        println!("  No differences - the feature matches v{version}.\n");
        return;
    }

    for line in &top {
        line.print();
    }
    print_section("tags", tags);
    print_section("variants", variants);
    print_section("segment overrides", segments);
    print_section("identity overrides", identities);
    print_section("rollout", rollout);
    println!();
}

fn tag_lines(current: &SnapshotState, target: &SnapshotState) -> Vec<Line> {
    let current_tags: HashSet<&str> = current.tags.iter().map(String::as_str).collect();
    let target_tags: HashSet<&str> = target.tags.iter().map(String::as_str).collect();

    let mut lines: Vec<Line> = current
        .tags
        .iter()
        .map(|t| {
            if target_tags.contains(t.as_str()) {
                Line::Same(t.clone())
            } else {
                Line::Removed(t.clone())
            }
        })
        .collect();

    lines.extend(
        target
            .tags
            .iter()
            .filter(|t| !current_tags.contains(t.as_str()))
            .map(|t| Line::Added(t.clone())),
    );

    lines
}

fn rollout_lines(current: &SnapshotState, target: &SnapshotState) -> Vec<Line> {
    let current_summary = rollout_summary(current);
    let target_summary = rollout_summary(target);

    match (current_summary, target_summary) {
        (None, None) => vec![],
        (Some(c), None) => vec![Line::Removed(c)],
        (None, Some(t)) => vec![Line::Added(t)],
        (Some(c), Some(t)) if c == t => vec![Line::Same(c)],
        (Some(c), Some(t)) => vec![Line::Changed { old: c, new: t }],
    }
}
