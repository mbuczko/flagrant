use anyhow::{anyhow, bail, Result};
use ulid::Ulid;

mod distributor;

enum FeatureValue<'a> {
    Simple(&'a String),
    Variadic(&'a Variation),
}

struct Feature<'a> {
    name: String,
    is_enabled: bool,
    control_value: String,
    distributor: Option<Box<dyn Distributor<'a>>>,
}



trait Distributor<'a> {
    fn distribute(&mut self, ident: Option<String>) -> Result<&'a Variation>;
}


impl<'a> Feature<'a> {
    fn new(name: String, value: String, is_enabled: bool) -> Result<Self> {
        Ok(Feature {
            name,
            is_enabled,
            control_value: value,
            distributor: None,
        })
    }
    fn new_variadic(
        name: String,
        control_value: String,
        is_enabled: bool,
        variations: Vec<(String, i16)>,
    ) -> Result<Self> {
        let distributor = AccumulativeDistribution::new(control_value.clone(), variations)?;
        Ok(Feature {
            name,
            is_enabled,
            control_value,
            distributor: Some(Box::new(distributor)),
        })
    }
    fn get_value(&self) -> Result<FeatureValue> {
        if let Some(vars) = self.variations {}
        match self.control_value {
            FeatureValue::Simple(ref val) => Ok(FeatureValue::Simple(val.clone())),
            FeatureValue::Variadic(ref mut strategy) => {
                let variation = strategy.distribute_and_fetch(None)?;
                Ok(FeatureValue::Variadic(variation))
            }
        }
    }
}



/// When a request is received:
/// - choose the variation with the largest `accum`
/// - subtract 100 from the `accum` for the chosen variation
/// - add `weight` to `accum` for all variations, including the chosen one
impl<'a> Distributor<'a> for AccumulativeDistribution {
    fn distribute(&mut self, _ident: Option<String>) -> Result<&'a Variation> {
        // let mut selected = None;
        let max_accum = self
            .variations
            .iter_mut()
            .max_by(|a, b| a.accum.cmp(&b.accum));

        if let Some(var) = max_accum {
            return Ok(var)
        }
        // if let Some(var) = max_accum {
        //     var.accum -= 100;
        //     selected = Some(var.clone());
        // }
        // if let Some(var) = selected {
        //     for i in self.variations.iter_mut() {
        //         i.accum += var.weight;
        //     }
        //     return Ok(max_accum.unwrap());
        // }
        Err(anyhow!("No variation found? Should not happen"))
    }
}

fn main() {
    println!("Hello, world!");
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn test_variadic_weights_eq_100_percent() {
        // let dist = AccumulativeDistribution::with_variations(
        //     String::from("control value"),
        //     Some(vec![(String::from("Big"), 40), (String::from("Small"), 20)]),
        // );
        // assert!(dist.is_ok());
        Feature::new(
            String::from("sample feature"),
            String::from("control value"),
            true,
        );
        assert!(Feature::new_variadic(
            String::from("sample feature"),
            String::from("control value"),
            true,
            vec![(String::from("Big"), 40), (String::from("Small"), 20)]
        )
        .is_ok())
    }

    // #[test]
    // fn test_variadic_weights_gt_100_percent() {
    //     let dist = AccumulativeDistribution::with_variations(
    //         String::from("control value"),
    //         Some(vec![(String::from("Big"), 60), (String::from("Small"), 50)]),
    //     );
    //     assert!(dist.is_err());
    // }

    // #[test]
    // fn test_variadic_buckets() {
    //     let dist = AccumulativeDistribution::with_variations(
    //         String::from("control value"),
    //         Some(vec![(String::from("Big"), 40), (String::from("Small"), 20)]),
    //     );
    //     let mut feature =
    //         Feature::create_variadic(String::from("sample feature"), dist.unwrap(), false)?;

    //     let buckets = HashMap::<String, String>::new();

    //     for i in 1..=100 {
    //         if let Ok(ulid) = feature.get_value() {
    //             buckets.put(ulid)
    //         }
    //     }
    // }
}
