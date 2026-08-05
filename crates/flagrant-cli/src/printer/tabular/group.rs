use colored::Colorize;
use fancy_table::{Align, FancyTable, FancyTableOpts, Layout, Overflow, TitleAlign, Width};

use super::Tabular;
use super::rule::subject_label;
use super::segment::format_connector;
use crate::{
    handlers::internal::effectives::{EffectiveGroup, EffectiveRule},
    printer::legend,
};

const UTF_TOP_CORNER: &str = "╭─";
const UTF_MIDDLE_BAR: &str = "│";
const UTF_BTM_CORNER: &str = "╰───";

impl Tabular for EffectiveGroup {
    type Patch = ();
    type Context = ();

    fn list(_: &[Self]) {}

    fn display(&self, _patch: Option<&()>, _ctx: &()) {
        let group = self;
        let group_num = group.label.strip_prefix("group-").unwrap_or(&group.label);
        let title = format!("GROUP #{group_num}").bold().to_string();

        let sym = group
            .connector
            .as_ref()
            .map(format_connector)
            .unwrap_or("(first group - no joiner)");

        let sym_colored = if group.is_deleted {
            sym.red().to_string()
        } else if sym.len() >= 10 {
            sym.dimmed().to_string()
        } else {
            sym.bright_cyan().to_string()
        };

        let (group_lines, group_staged) = group_body_lines(group, false);
        let group_str = group_lines.join("\n");
        let has_staged = group.is_deleted || group_staged;
        let nlines = group_lines.len();

        let table = FancyTable::create(FancyTableOpts::default())
            .add_column(None, Layout::Fixed(16), Align::Right, Overflow::Truncate, 1)
            .add_column(
                None,
                Layout::Expandable(100),
                Align::Left,
                Overflow::Truncate,
                nlines,
            )
            .hseparator(Some(fancy_table::Separator::Custom('-')))
            .add_title_with_align(title.as_str(), TitleAlign::LeftOffset(5))
            .width(Width::Percentage(100))
            .build();
        table.render(vec![
            vec!["joiner".to_string(), sym_colored],
            vec!["group".to_string(), group_str],
        ]);

        legend::print_footer("", has_staged);
    }
}

/// Renders a group's own body: its label/description header, its rules (or a placeholder if
/// it has none), and the closing corner - everything except the leading connector/joiner
/// symbol, which is shown differently by each caller (inlined by `Segment::display`, shown as
/// its own row by `EffectiveGroup::display`). Also returns whether *any* line is staged.
///
/// `force_deleted` lets a caller (namely `Segment::display`, when the whole segment is staged
/// for deletion) render every group as deleting too, regardless of the group's own state -
/// since a segment deletion takes every one of its groups down with it.
pub(super) fn group_body_lines(group: &EffectiveGroup, force_deleted: bool) -> (Vec<String>, bool) {
    let is_deleted = group.is_deleted || force_deleted;
    let mut lines: Vec<String> = Vec::new();
    let mut staged = is_deleted || group.is_staged_add || group.description_modified;

    let label_colored = if is_deleted {
        group.label.red()
    } else if group.is_staged_add {
        group.label.green()
    } else {
        group.label.yellow()
    };
    let desc_part = group
        .description
        .as_deref()
        .map(|d| {
            let d_colored = if is_deleted {
                d.red()
            } else if group.description_modified {
                d.yellow()
            } else if group.is_staged_add {
                d.green()
            } else {
                d.dimmed()
            };
            format!(" {} {}", "─".dimmed(), d_colored)
        })
        .unwrap_or_default();

    lines.push(format!(
        "{} {label_colored}{desc_part}",
        UTF_TOP_CORNER.dimmed()
    ));

    if group.rules.is_empty() {
        lines.push(format!(
            "{}  {}",
            UTF_MIDDLE_BAR.dimmed(),
            "(no rules)".dimmed()
        ));
    } else {
        let max_subject = group
            .rules
            .iter()
            .map(|r| subject_label(&r.subject).len())
            .max()
            .unwrap_or(0);

        let mut display_idx = 1usize;

        for r in &group.rules {
            let (line, rule_staged) = rule_line(r, display_idx, is_deleted, max_subject);
            lines.push(line);
            staged |= rule_staged;

            if !r.is_staged_add {
                display_idx += 1;
            }
        }
    }

    lines.push(UTF_BTM_CORNER.dimmed().to_string());

    (lines, staged)
}

/// Renders one rule as a single compact colored line, used both when a group is shown
/// standalone and when a segment inlines every one of its groups' rules. Also returns
/// whether the rule is currently staged.
///
/// `display_idx` is the 1-based position to show (tracked by the caller since it depends on
/// position within the group's rule list, not on the rule alone - staged-add rules show "+"
/// instead of an index).
fn rule_line(
    rule: &EffectiveRule,
    display_idx: usize,
    group_deleted: bool,
    max_subject: usize,
) -> (String, bool) {
    let subject = subject_label(&rule.subject);
    let cmp = rule.comparator.to_string();

    let (pipe, idx_str, subject_s, cmp_s, val_s, staged) = if group_deleted || rule.is_deleted {
        (
            UTF_MIDDLE_BAR.dimmed(),
            display_idx.to_string().red(),
            subject.red(),
            cmp.red(),
            rule.value.as_str().red(),
            rule.is_deleted,
        )
    } else if rule.is_staged_add {
        (
            UTF_MIDDLE_BAR.dimmed(),
            "+".green(),
            subject.bright_blue(),
            cmp.dimmed(),
            rule.value.as_str().green(),
            true,
        )
    } else if rule.value_modified || rule.comparator_modified {
        (
            UTF_MIDDLE_BAR.dimmed(),
            display_idx.to_string().dimmed(),
            subject.bright_blue(),
            if rule.comparator_modified {
                cmp.yellow()
            } else {
                cmp.dimmed()
            },
            if rule.value_modified {
                rule.value.as_str().yellow()
            } else {
                rule.value.as_str().green()
            },
            true,
        )
    } else {
        (
            UTF_MIDDLE_BAR.dimmed(),
            display_idx.to_string().dimmed(),
            subject.bright_blue(),
            cmp.dimmed(),
            rule.value.as_str().green(),
            false,
        )
    };

    let line = format!(
        "{pipe}  {idx_str}  {subject_s:<dw$}  {cmp_s}  {val_s}",
        dw = max_subject
    );
    (line, staged)
}
