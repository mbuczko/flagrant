use anyhow::Result;
use distributor::{AccumulativeDistributor, Distributor, Variation};

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
        let mut distributor = AccumulativeDistributor::new(control_value.clone());
        let vars = variations
            .into_iter()
            .map(|(value, weight)| Variation {
                id: None,
                value,
                weight,
            })
            .collect::<Vec<_>>();

        distributor.set_variations(vars);
        Ok(Feature {
            name,
            is_enabled,
            control_value,
            distributor: Some(Box::new(distributor)),
        })
    }
    fn get_value(&mut self, ident: Option<String>) -> Result<&String> {
        if let Some(distributor) = &mut self.distributor {
            return Ok(&distributor.distribute(ident)?.value);
        }
        Ok(&self.control_value)
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
