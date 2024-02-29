use anyhow::{anyhow, Result};
use distributor::{AccumulativeDistributor, Distributor, Variation};
use ulid::Ulid;

pub mod distributor;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FeatureValue<'a> {
    Simple(&'a String),
    Variadic(&'a Variation),
}

#[derive(Debug)]
pub struct Feature<'a> {
    name: String,
    is_enabled: bool,
    control_value: String,
    distributor: Option<Box<dyn Distributor<'a>>>,
}

impl<'a> Feature<'a> {
    pub fn new(name: String, value: String) -> Result<Self> {
        Ok(Feature {
            name,
            is_enabled: false,
            control_value: value,
            distributor: None,
        })
    }

    pub fn new_variadic(
        name: String,
        control_value: String,
        variations: Vec<(String, i16)>,
    ) -> Result<Self> {
        let mut distributor = AccumulativeDistributor::new(control_value.clone());
        let vars = variations
            .into_iter()
            .map(|(value, weight)| Variation {
                id: Ulid::new(),
                value,
                weight,
            })
            .collect::<Vec<_>>();

        distributor.set_variations(vars)?;
        Ok(Feature {
            name,
            is_enabled: false,
            control_value,
            distributor: Some(Box::new(distributor)),
        })
    }

    pub fn variation(&mut self, id: Option<Ulid>) -> Option<&Variation> {
        if let Some(distributor) = &mut self.distributor {
            if let Some(id) = id {
                return distributor
                    .variations()
                    .into_iter()
                    .find(|v| v.id == id);
            }
            return Some(distributor.distribute());
        }
        None
    }

    pub fn value(&mut self, id: Option<Ulid>) -> FeatureValue {
        if self.distributor.is_some() {
            return FeatureValue::Variadic(self.variation(id).unwrap());
        }
        FeatureValue::Simple(&self.control_value)
    }

    pub fn variations(&self) -> Result<Vec<&Variation>> {
        if let Some(distributor) = &self.distributor {
            Ok(distributor.variations())
        } else {
            Err(anyhow!("Not a variadic feature"))
        }
    }
}
