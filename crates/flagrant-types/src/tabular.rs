use crate::{Feature, Variant};

pub trait Tabular {
    fn tabular_print(&self);
}

impl Tabular for Feature {
    fn tabular_print(&self) {
        let toggle = if self.is_enabled { "▣" } else { "▢" };
        let value = self.get_default_variant().map(|v| &v.value);

        println!(
            "│ {:<8}: {}\n│ {:<8}: {}\n│ {:<8}: {toggle} {}\n│ {:<8}: {}",
            "ID",
            self.id,
            "NAME",
            self.name,
            "ENABLED",
            self.is_enabled,
            "VALUE",
            value
                .map(|v| v.to_string())
                .unwrap_or_default()
        )
    }
}

impl Tabular for Variant {
    fn tabular_print(&self) {
        println!(
            "│ {:<8}: {}\n│ {:<8}: {}\n│ {:<8}: {}",
            "ID", self.id, "WEIGHT", self.weight, "VALUE", self.value
        )
    }
}
