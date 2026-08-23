use colored::Colorize;
use fancy_table::{Align, FancyTable, FancyTableOpts, Layout, Overflow, TitleAlign, Width};
use flagrant_types::FeatureResponse;

use super::Tabular;

impl Tabular for FeatureResponse {
    type Patch = ();
    type Context = ();

    fn list(selfs: &[Self]) {
        if selfs.is_empty() {
            println!("No features found.");
            return;
        }
        let rows: Vec<_> = selfs
            .iter()
            .map(|f| {
                let state = if f.is_enabled {
                    format!("{} ON", "●".green())
                } else {
                    format!("{} OFF", "●".red())
                };
                let (typ, val) = f.value.decompose();
                [f.name.clone(), state, typ.to_string(), val.to_string()]
            })
            .collect();

        FancyTable::create(FancyTableOpts::default())
            .add_column_named_with_align("NAME".into(), Layout::Fixed(30), Align::Left)
            .add_column_named_with_align("STATUS".into(), Layout::Fixed(8), Align::Left)
            .add_column_named_with_align("TYPE".into(), Layout::Fixed(6), Align::Left)
            .add_column_named_wrapping_with_align(
                "VALUE".into(),
                Layout::Expandable(60),
                Align::Left,
            )
            .width(Width::Percentage(100))
            .build()
            .render(rows);
    }

    fn display(&self, _patch: Option<&()>, _ctx: &()) {
        let state = if self.is_enabled {
            format!("{} ON", "●".green())
        } else {
            format!("{} OFF", "●".red())
        };
        let title = "FEATURE".bold().to_string();
        let table = FancyTable::create(FancyTableOpts::default())
            .add_column(None, Layout::Fixed(20), Align::Right, Overflow::Truncate, 1)
            .add_column(
                None,
                Layout::Expandable(120),
                Align::Left,
                Overflow::Wrap,
                10,
            )
            .hseparator(Some(fancy_table::Separator::Custom('-')))
            .add_title_with_align(&title, TitleAlign::LeftOffset(10))
            .width(Width::Percentage(100))
            .build();

        let (typ, val) = self.value.decompose();

        table.render(vec![
            vec!["name".to_string(), self.name.clone()],
            vec!["status".to_string(), state],
            vec!["type".to_string(), typ.to_string()],
            vec!["value".to_string(), val.to_string()],
        ]);
    }
}
