use ascii_table::AsciiTable;
use flagrant_types::{Environment, Feature, Variant};

pub trait Tabular {
    fn tabular_print(&self);
}

impl Tabular for Environment {
    fn tabular_print(&self) {
        let id_str = self.id.to_string();
        let desc_str = self.description.as_deref().unwrap_or("");

        let mut table = AsciiTable::default();
        let vec = vec![
            vec!["ID", &id_str],
            vec!["NAME", &self.name],
            vec!["DESCRIPTION", &desc_str],
        ];

        table.column(0).set_align(ascii_table::Align::Right);
        table.print(vec);
    }
}

impl Tabular for Feature {
    fn tabular_print(&self) {
        let toggle = if self.is_enabled { "▣" } else { "▢" };
        let value = self.get_default_value();

        let id_str = self.id.to_string();
        let tgl_str = format!("{toggle} {}", self.is_enabled);
        let val_str = value
            .map(|v| v.to_string())
            .unwrap_or_default();

        let mut table = AsciiTable::default();
        let vec = vec![
            vec!["ID", &id_str],
            vec!["NAME", &self.name],
            vec!["ENABLED", &tgl_str],
            vec!["VALUE", &val_str],
        ];

        table.column(0).set_align(ascii_table::Align::Right);
        table.print(vec);
    }
}

impl Tabular for Variant {
    fn tabular_print(&self) {
        let id_str = self.id.to_string();
        let wgh_str = self.weight.to_string();
        let val_str = self.value.to_string();

        let mut table = AsciiTable::default();
        let vec = vec![
            vec!["ID", &id_str],
            vec!["WEIGHT", &wgh_str],
            vec!["VALUE", &val_str],
        ];

        table.column(0).set_align(ascii_table::Align::Right);
        table.print(vec);
    }
}
