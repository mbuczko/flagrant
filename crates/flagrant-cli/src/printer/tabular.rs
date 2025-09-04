use colored::Colorize;
use fancy_table::{Align, FancyTable, FancyTableOpts, Layout, Overflow, TitleAlign};
use flagrant_types::{Environment, Feature, Variant};

pub trait Tabular {
    fn list(rows: &[Self])
    where
        Self: Sized;
    fn describe(&self);
}

impl Tabular for Environment {
    fn list(selfs: &[Self]) {
        let rows: Vec<_> = selfs
            .iter()
            .map(|env| {
                [
                    env.name.clone(),
                    env.description.clone().unwrap_or_default(),
                ]
            })
            .collect();

        FancyTable::create(FancyTableOpts::default())
            .add_column_named_with_align("NAME".into(), Layout::Fixed(30), Align::Left)
            .add_column_named_with_align("DESCRIPTION".into(), Layout::Expandable(100), Align::Left)
            .rseparator(None)
            .build()
            .render(rows);
    }
    fn describe(&self) {
        let id_str = self.id.to_string();
        let desc_str = self.description.as_deref().unwrap_or("");

        let table = FancyTable::create(FancyTableOpts::default())
            .add_column(None, Layout::Fixed(6), Align::Right, Overflow::Truncate, 1)
            .add_column(
                None,
                Layout::Expandable(120),
                Align::Left,
                Overflow::Truncate,
                1,
            )
            .hseparator(Some(fancy_table::Separator::Custom('-')))
            .add_title_with_align("ENVIRONMENT", TitleAlign::RightOffset(1))
            .build();

        table.render(vec![
            &["ID", &id_str],
            &["NAME", &self.name],
            &["DESCRIPTION", desc_str],
        ]);
    }
}

impl Tabular for Feature {
    fn list(selfs: &[Self]) {
        let rows = selfs
            .iter()
            .map(|feat| {
                let value = feat.get_default_value().to_string();
                let toggle = if feat.is_enabled {
                    format!("{} ON", "●".green())
                } else {
                    format!("{} OFF", "●".red())
                };
                [feat.name.clone(), toggle, value]
            })
            .collect();

        FancyTable::create(FancyTableOpts::default())
            .add_column_named_with_align("NAME".into(), Layout::Fixed(30), Align::Left)
            .add_column_named_with_align("STATE".into(), Layout::Slim, Align::Center)
            .add_column_named_with_align("VALUE".into(), Layout::Expandable(100), Align::Left)
            .build()
            .render(rows)
    }
    fn describe(&self) {
        let value = self.get_default_value();
        let title = format!("{} (ID={})", &self.name, self.id);
        let table = FancyTable::create(FancyTableOpts::default())
            .add_column(None, Layout::Fixed(10), Align::Right, Overflow::Truncate, 1)
            .add_column(
                None,
                Layout::Expandable(120),
                Align::Left,
                Overflow::Truncate,
                3,
            )
            .add_title_with_align(title.as_str(), TitleAlign::RightOffset(1))
            .build();

        let toggle = if self.is_enabled {
            format!("{} ON", "●".green())
        } else {
            format!("{} OFF", "●".red())
        };

        table.render(vec![&["STATUS", &toggle], &["VALUE", &value.to_string()]]);
    }
}

impl Tabular for Variant {
    fn list(selfs: &[Self]) {
        let rows = selfs
            .iter()
            .map(|var| [bar(var.weight, 10), var.value.to_string()])
            .collect();

        FancyTable::create(FancyTableOpts::default())
            .add_column_named_with_align("WEIGHT".into(), Layout::Fixed(18), Align::Left)
            .add_column_named_with_align("VALUE".into(), Layout::Expandable(120), Align::Left)
            .build()
            .render(rows)
    }
    fn describe(&self) {
        let id_str = self.id.to_string();
        let wgh_str = self.weight.to_string();
        let val_str = self.value.to_string();

        let table = FancyTable::create(FancyTableOpts::default())
            .add_column(None, Layout::Fixed(10), Align::Right, Overflow::Truncate, 1)
            .add_column(
                None,
                Layout::Expandable(120),
                Align::Left,
                Overflow::Truncate,
                1,
            )
            .add_title_with_align("VARIANT", TitleAlign::RightOffset(1))
            .build();

        table.render(vec![
            &["ID", &id_str],
            &["WEIGHT", &wgh_str],
            &["VALUE", &val_str],
        ]);
    }
}

fn bar(weight: u8, width: u16) -> String {
    let mut bar = vec![' '; width as usize];
    let progress = (weight as u16 * width) / 100;

    for ch in bar.iter_mut().take(progress as usize) {
        *ch = '▆';
    }
    format!("{0: <3}% {1: <10}", weight, String::from_iter(bar))
}
