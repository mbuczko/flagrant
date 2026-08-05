use colored::Colorize;
use fancy_table::{Align, FancyTable, FancyTableOpts, Layout, Overflow, TitleAlign, Width};
use flagrant_types::Subject;

use super::Tabular;
use crate::{handlers::internal::effectives::EffectiveRule, printer::legend};

pub(super) fn subject_label(subject: &Subject) -> String {
    match subject {
        Subject::Identity => "identity".to_string(),
        Subject::Trait(name) => format!("trait:{name}"),
        Subject::Environment => "environment".to_string(),
    }
}

impl Tabular for EffectiveRule {
    type Patch = ();
    type Context = (String, usize);

    fn list(_: &[Self]) {}

    fn display(&self, _patch: Option<&()>, ctx: &(String, usize)) {
        let rule = self;
        let (group_label, index) = ctx;
        let title = format!("RULE #{index}").bold().to_string();

        let (subject_s, comparator_s, value_s) = if rule.is_deleted {
            (
                subject_label(&rule.subject).red(),
                rule.comparator.to_string().red(),
                rule.value.red(),
            )
        } else if rule.is_staged_add {
            (
                subject_label(&rule.subject).green(),
                rule.comparator.to_string().green(),
                rule.value.as_str().green(),
            )
        } else {
            (
                subject_label(&rule.subject).bright_blue(),
                if rule.comparator_modified {
                    rule.comparator.to_string().yellow()
                } else {
                    rule.comparator.to_string().dimmed()
                },
                if rule.value_modified {
                    rule.value.as_str().yellow()
                } else {
                    rule.value.as_str().green()
                },
            )
        };

        let has_staged = rule.is_deleted
            || rule.is_staged_add
            || rule.comparator_modified
            || rule.value_modified;

        let table = FancyTable::create(FancyTableOpts::default())
            .add_column(None, Layout::Fixed(16), Align::Right, Overflow::Truncate, 1)
            .add_column(
                None,
                Layout::Expandable(100),
                Align::Left,
                Overflow::Wrap,
                10,
            )
            .hseparator(Some(fancy_table::Separator::Custom('-')))
            .add_title_with_align(title.as_str(), TitleAlign::LeftOffset(6))
            .width(Width::Percentage(100))
            .build();
        table.render(vec![
            vec!["group".to_string(), group_label.clone()],
            vec!["subject".to_string(), subject_s.to_string()],
            vec!["comparator".to_string(), comparator_s.to_string()],
            vec!["value".to_string(), value_s.to_string()],
        ]);

        legend::print_footer("", has_staged);
    }
}
