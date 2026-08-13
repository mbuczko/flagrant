use colored::Colorize;
use fancy_table::{Align, FancyTable, FancyTableOpts, Layout, Overflow, TitleAlign, Width};

use super::Tabular;
use crate::{handlers::internal::effectives::EffectiveVariant, printer::legend};

/// Generous enough to cover multi-line JSON/TOML values without truncating.
const VALUE_MAX_LINES: usize = 20;

impl Tabular for EffectiveVariant {
    type Patch = ();
    type Context = (usize, Vec<String>);

    fn list(_: &[Self]) {}

    fn display(&self, _patch: Option<&()>, ctx: &(usize, Vec<String>)) {
        let variant = self;
        let (index, identities) = ctx;
        let title = format!("VARIANT #{index}").bold().to_string();

        let (type_str, bare_value) = variant.value.decompose();

        let value_s = legend::stage_color(
            bare_value,
            variant.is_deleted,
            variant.is_staged_add,
            variant.value_modified,
        )
        .into_owned();

        let type_s = type_str.dimmed().to_string();

        let weight_str = format!("{}%", variant.weight);
        let weight_s = legend::stage_color(
            weight_str,
            variant.is_deleted,
            variant.is_staged_add,
            variant.weight_modified,
        )
        .into_owned();

        let control_s = if variant.is_control {
            "yes".to_string()
        } else {
            "no".dimmed().to_string()
        };

        let identities_s = if identities.is_empty() {
            "(none)".dimmed().to_string()
        } else {
            identities.join(", ")
        };

        let has_staged = variant.is_deleted
            || variant.is_staged_add
            || variant.value_modified
            || variant.weight_modified;

        let table = FancyTable::create(FancyTableOpts::default())
            .add_column(None, Layout::Fixed(20), Align::Right, Overflow::Truncate, 1)
            .add_column(
                None,
                Layout::Expandable(100),
                Align::Left,
                Overflow::Wrap,
                VALUE_MAX_LINES,
            )
            .hseparator(Some(fancy_table::Separator::Custom('-')))
            .add_title_with_align(title.as_str(), TitleAlign::LeftOffset(7))
            .width(Width::Percentage(100))
            .build();

        table.render(vec![
            vec!["weight".to_string(), weight_s],
            vec!["control".to_string(), control_s],
            vec!["value".to_string(), value_s],
            vec!["value type".to_string(), type_s],
            vec!["pinned identities".to_string(), identities_s],
        ]);

        legend::print_footer("", has_staged);
    }
}
