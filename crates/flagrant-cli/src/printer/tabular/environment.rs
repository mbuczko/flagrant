use colored::Colorize;
use fancy_table::{Align, FancyTable, FancyTableOpts, Layout, Overflow, TitleAlign, Width};
use flagrant_types::Environment;

use crate::printer::legend;

use super::Tabular;

impl Tabular for Environment {
    type Patch = ();
    type Context = ();

    fn list(selfs: &[Self]) {
        list_with_current(selfs, None);
    }

    fn display(&self, _patch: Option<&()>, _ctx: &()) {
        let desc_str = self.description.as_deref().unwrap_or("");
        let title = "ENVIRONMENT".bold().to_string();
        let table = FancyTable::create(FancyTableOpts::default())
            .add_column(None, Layout::Fixed(20), Align::Right, Overflow::Truncate, 1)
            .add_column(
                None,
                Layout::Expandable(120),
                Align::Left,
                Overflow::Truncate,
                1,
            )
            .hseparator(Some(fancy_table::Separator::Custom('-')))
            .add_title_with_align(title.as_str(), TitleAlign::LeftOffset(6))
            .width(Width::Percentage(100))
            .build();

        table.render(vec![&["name", &self.name], &["description", desc_str]]);
    }
}

/// Renders the environment list table, marking `current`'s row with a star and
/// explaining it in a footer legend below (if given). `Tabular::list`'s signature has no
/// room for "which one is current" - environments are the only type needing to flag that
/// in a list view - so `ENVIRONMENT list` calls this directly instead of going through
/// the trait; `Tabular::list` itself just delegates here with `current: None`.
pub(crate) fn list_with_current(envs: &[Environment], current: Option<&str>) {
    if envs.is_empty() {
        println!("No environments found.");
        return;
    }
    let rows: Vec<_> = envs
        .iter()
        .map(|env| {
            let name = if current == Some(env.name.as_str()) {
                format!("{} {}", env.name, "★".dimmed())
            } else {
                env.name.clone()
            };
            [name, env.description.clone().unwrap_or_default()]
        })
        .collect();

    FancyTable::create(FancyTableOpts::default())
        .add_column_named_with_align("NAME".into(), Layout::Fixed(30), Align::Left)
        .add_column_named_with_align("DESCRIPTION".into(), Layout::Expandable(100), Align::Left)
        .rseparator(None)
        .width(Width::Percentage(100))
        .build()
        .render(rows);

    if current.is_some() {
        legend::print_footer(&format!(" {} current environment", "★".dimmed()), false);
    }
}
