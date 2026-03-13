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
            .width(100)
            .build()
            .render(rows);
    }
    fn describe(&self) {
        let desc_str = self.description.as_deref().unwrap_or("");
        let title = format!("Environment: {} (ID={})", self.name, self.id);
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
            .add_title_with_align(title.as_str(), TitleAlign::RightOffset(1))
            .build();

        table.render(vec![&["NAME", &self.name], &["DESCRIPTION", desc_str]]);
    }
}

impl Tabular for Feature {
    fn list(selfs: &[Self]) {
        let rows = selfs
            .iter()
            .map(|feat| {
                let tags = feat.tags.to_string();
                let value = feat.get_default_value().to_string();
                let state = if feat.is_enabled {
                    format!("{} ON", "●".green())
                } else {
                    format!("{} OFF", "●".red())
                };
                let status = if feat.is_active {
                    String::from("active")
                } else {
                    format!("{}", "inactive".dimmed())
                };
                [feat.name.clone(), status, state, value, tags]
            })
            .collect();

        FancyTable::create(FancyTableOpts::default())
            .add_column_named_with_align("NAME".into(), Layout::Fixed(30), Align::Left)
            .add_column_named_with_align("STATUS".into(), Layout::Fixed(10), Align::Center)
            .add_column_named_with_align("STATE".into(), Layout::Slim, Align::Center)
            .add_column_named_with_align("VALUE".into(), Layout::Expandable(30), Align::Left)
            .add_column_named_with_align("TAGS".into(), Layout::Expandable(20), Align::Left)
            .width(100)
            .build()
            .render(rows)
    }
    fn describe(&self) {
        let title = format!("{} (ID={})", &self.name, self.id);
        let tags = format!("{}", self.tags.to_string().bright_blue());
        let table = FancyTable::create(FancyTableOpts::default())
            .add_column(None, Layout::Fixed(10), Align::Right, Overflow::Truncate, 1)
            .add_column(
                None,
                Layout::Expandable(120),
                Align::Left,
                Overflow::Truncate,
                10,
            )
            .width(100)
            .add_title_with_align(title.as_str(), TitleAlign::RightOffset(1))
            .build();

        let vcount = self.variants.len();
        let variants = self
            .variants
            .iter()
            .enumerate()
            .map(|(i, v)| {
                format!(
                    "{}{} {}",
                    if i == vcount - 1 { "╰╴" } else { "├╴" },
                    bar(v.weight, 10),
                    v.value,
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let state = if self.is_enabled {
            format!("{} ON", "●".green())
        } else {
            format!("{} OFF", "●".red())
        };
        let status = if self.is_active {
            "active".to_string()
        } else {
            "inactive".dimmed().to_string()
        };
        table.render(vec![
            &["STATUS", &status],
            &["STATE", &state],
            &["VARIANTS", &variants],
            &["TAGS", &tags],
        ]);
    }
}

/// A single row for the variant listing table.
/// All strings are pre-colored by the caller.
/// When `state` is `None` the STATE column is omitted entirely.
pub struct VariantRow {
    pub index: String,
    pub weight: String,
    pub value: String,
    pub state: Option<String>,
}

impl VariantRow {
    pub fn committed(index: usize, var: &Variant) -> Self {
        VariantRow {
            index: if var.is_control() {
                format!("{}★", index)
            } else {
                index.to_string()
            },
            weight: bar(var.weight, 10),
            value: var.value.to_string(),
            state: None,
        }
    }
}

/// Render a variant listing table, consuming the rows to avoid cloning.
///
/// If any row carries a `state`, a STATE column is added for all rows.
pub fn variant_list(rows: Vec<VariantRow>) {
    let with_state = rows.iter().any(|r| r.state.is_some());

    if with_state {
        let data: Vec<[String; 4]> = rows
            .into_iter()
            .map(|r| [r.index, r.weight, r.value, r.state.unwrap_or_default()])
            .collect();

        FancyTable::create(FancyTableOpts::default())
            .add_column_named_with_align("#".into(), Layout::Fixed(4), Align::Left)
            .add_column_named_with_align("WEIGHT".into(), Layout::Fixed(18), Align::Left)
            .add_column_named_with_align("VALUE".into(), Layout::Expandable(80), Align::Left)
            .add_column_named_with_align("STATE".into(), Layout::Fixed(10), Align::Left)
            .width(100)
            .build()
            .render(data);
    } else {
        let data: Vec<[String; 3]> = rows
            .into_iter()
            .map(|r| [r.index, r.weight, r.value])
            .collect();

        FancyTable::create(FancyTableOpts::default())
            .add_column_named_with_align("#".into(), Layout::Fixed(4), Align::Left)
            .add_column_named_with_align("WEIGHT".into(), Layout::Fixed(18), Align::Left)
            .add_column_named_with_align("VALUE".into(), Layout::Expandable(80), Align::Left)
            .width(100)
            .build()
            .render(data);
    }

    println!("  {} control variant\n", "★".dimmed());
}

pub fn bar(weight: u8, width: u16) -> String {
    let total_halves = weight as u32 * width as u32 * 2 / 100;
    let full_chars = (total_halves / 2) as usize;
    let half = total_halves % 2 == 1;
    let filled = full_chars + half as usize;

    let mut bar = String::with_capacity(width as usize);
    for _ in 0..full_chars {
        bar.push('━');
    }
    if half {
        bar.push('╸');
    }
    for _ in filled..width as usize {
        bar.push(' ');
    }
    format!("{0: <3}% {1: <10}", weight, bar)
}
