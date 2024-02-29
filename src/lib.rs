use anyhow::Result;
use distributor::{AccumulativeDistributor, Distributor, Variation};
use ulid::Ulid;

mod distributor;

enum FeatureValue<'a> {
    Simple(&'a String),
    Variadic(&'a Variation),
}

pub struct Feature<'a> {
    name: String,
    is_enabled: bool,
    control_value: String,
    distributor: Option<Box<dyn Distributor<'a>>>,
}

impl<'a> Feature<'a> {
    pub fn new(name: String, value: String, is_enabled: bool) -> Result<Self> {
        Ok(Feature {
            name,
            is_enabled,
            control_value: value,
            distributor: None,
        })
    }
    pub fn new_variadic(
        name: String,
        control_value: String,
        is_enabled: bool,
        variations: Vec<(String, i16)>,
    ) -> Result<Self> {
        let mut distributor = AccumulativeDistributor::new(control_value.clone());
        let vars = variations
            .into_iter()
            .map(|(value, weight)| Variation {
                id: None,
                value,
                weight,
            })
            .collect::<Vec<_>>();

        distributor.set_variations(vars)?;
        Ok(Feature {
            name,
            is_enabled,
            control_value,
            distributor: Some(Box::new(distributor)),
        })
    }
    pub fn get_value(&mut self, ident: Option<String>) -> Result<(&Option<Ulid>, &String)> {
        if let Some(distributor) = &mut self.distributor {
            let result = distributor.distribute(ident)?;
            return Ok((&result.id, &result.value));
        }
        Ok((&None, &self.control_value))
    }
}
