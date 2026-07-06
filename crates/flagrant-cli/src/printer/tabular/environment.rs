use fancy_table::{Align, FancyTable, FancyTableOpts, Layout, Overflow, TitleAlign};
use flagrant_types::Environment;

use super::Tabular;

impl Tabular for Environment {
    type Patch = ();
    type Context = ();

    fn list(selfs: &[Self]) {
        let rows: Vec<_> = selfs
            .iter()
            .map(|env| [env.name.clone(), env.description.clone().unwrap_or_default()])
            .collect();

        FancyTable::create(FancyTableOpts::default())
            .add_column_named_with_align("NAME".into(), Layout::Fixed(30), Align::Left)
            .add_column_named_with_align("DESCRIPTION".into(), Layout::Expandable(100), Align::Left)
            .rseparator(None)
            .width(100)
            .build()
            .render(rows);
    }

    fn describe(&self, _patch: Option<&()>, _ctx: &()) {
        let desc_str = self.description.as_deref().unwrap_or("");
        let title = format!("Environment: {} (ID={})", self.name, self.id);
        let table = FancyTable::create(FancyTableOpts::default())
            .add_column(None, Layout::Fixed(6), Align::Right, Overflow::Truncate, 1)
            .add_column(None, Layout::Expandable(120), Align::Left, Overflow::Truncate, 1)
            .hseparator(Some(fancy_table::Separator::Custom('-')))
            .add_title_with_align(title.as_str(), TitleAlign::RightOffset(1))
            .build();

        table.render(vec![&["NAME", &self.name], &["DESCRIPTION", desc_str]]);
    }
}
