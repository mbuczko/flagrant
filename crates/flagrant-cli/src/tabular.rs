use fancy_table::{Align, FancyTable, FancyTableOpts, Layout, Overflow};
use flagrant_types::{Environment, Feature, Variant};

pub trait Tabular {
    fn table() -> FancyTable<'static, String>;
    fn render(&self);
}

impl Tabular for Environment {
    fn table() -> FancyTable<'static, String> {
        FancyTable::create(FancyTableOpts::default())
            .add_column_named_with_align("ID".into(), Layout::Fixed(6), Align::Left)
            .add_column_named_with_align("NAME".into(), Layout::Expandable(50), Align::Left)
            .add_column_named_with_align("DESCRIPTION".into(), Layout::Expandable(100), Align::Left)
            .rseparator(None)
            .build(80)
    }
    fn render(&self) {
        let id_str = self.id.to_string();
        let desc_str = self.description.as_deref().unwrap_or("");

        let table = FancyTable::create(FancyTableOpts::default())
            .add_column(6, 1, Layout::Fixed(6), Align::Right, Overflow::Truncate)
            .add_column(
                70,
                1,
                Layout::Expandable(120),
                Align::Left,
                Overflow::Truncate,
            )
            .hseparator(Some(fancy_table::Separator::Custom('-')))
            .build(80);

        table.render(vec![
            &["ID", &id_str],
            &["NAME", &self.name],
            &["DESCRIPTION", desc_str],
        ]);
    }
}

impl Tabular for Feature {
    fn table() -> FancyTable<'static, String> {
        FancyTable::create(FancyTableOpts::default())
            .add_column_named_with_align("ID".into(), Layout::Fixed(6), Align::Left)
            .add_column_named_with_align("NAME".into(), Layout::Expandable(50), Align::Left)
            .add_column_named_with_align("ENABLED?".into(), Layout::Fixed(10), Align::Center)
            .add_column_named_with_align("VALUE".into(), Layout::Expandable(100), Align::Left)
            .build(80)
    }
    fn render(&self) {
        let toggle = if self.is_enabled { "▣" } else { "▢" };
        let value = self.get_default_value();

        let id_str = self.id.to_string();
        let tgl_str = format!("{toggle} {}", self.is_enabled);
        let val_str = value.map(|v| v.to_string()).unwrap_or_default();

        let table = FancyTable::create(FancyTableOpts::default())
            .add_column(6, 1, Layout::Fixed(6), Align::Right, Overflow::Truncate)
            .add_column(
                70,
                1,
                Layout::Expandable(120),
                Align::Left,
                Overflow::Truncate,
            )
            .build(80);

        table.render(vec![
            &["ID", &id_str],
            &["NAME", &self.name],
            &["ENABLED", &tgl_str],
            &["VALUE", &val_str],
        ]);
    }
}

impl Tabular for Variant {
    fn table() -> FancyTable<'static, String> {
        FancyTable::create(FancyTableOpts::default())
            .add_column_named_with_align("ID".into(), Layout::Fixed(6), Align::Left)
            .add_column_named_with_align("WEIGHT".into(), Layout::Fixed(8), Align::Left)
            .add_column_named_with_align("VALUE".into(), Layout::Expandable(120), Align::Left)
            .build(80)
    }
    fn render(&self) {
        let id_str = self.id.to_string();
        let wgh_str = self.weight.to_string();
        let val_str = self.value.to_string();

        let table = FancyTable::create(FancyTableOpts::default())
            .add_column(6, 1, Layout::Fixed(6), Align::Right, Overflow::Truncate)
            .add_column(70, 1, Layout::Expandable(120), Align::Left, Overflow::Truncate)
            .build(80);

        table.render(vec![
            &["ID", &id_str],
            &["WEIGHT", &wgh_str],
            &["VALUE", &val_str],
        ]);
    }
}
