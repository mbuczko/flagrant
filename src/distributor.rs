use anyhow::{bail, Result};
use ulid::Ulid;

#[derive(Clone, Debug)]
pub struct Variation {
    pub id: Option<Ulid>,
    pub value: String,
    pub weight: i16,
}

#[derive(Clone, Debug)]
pub struct AccumulatedVar {
    accum: i16,
    variation: Variation,
}

#[derive(Debug)]
pub struct AccumulativeDistributor {
    requests: usize,
    variations: Vec<AccumulatedVar>,
}

pub trait Distributor<'a> {
    fn distribute(&mut self, ident: Option<String>) -> &Variation;
    fn variations(&self) -> Vec<&Variation>;

    fn set_control_value(&'a mut self, value: String) -> Result<Vec<&'a Variation>>;
    fn set_variations(&'a mut self, variations: Vec<Variation>) -> Result<Vec<&'a Variation>>;
}

impl AccumulativeDistributor {
    pub fn new(control_value: String) -> Self {
        let accumulated = AccumulatedVar {
            accum: 100,
            variation: Variation {
                id: Some(Ulid::new()),
                value: control_value,
                weight: 100,
            },
        };
        Self {
            requests: 0,
            variations: vec![accumulated],
        }
    }
}

impl<'a> Distributor<'a> for AccumulativeDistributor {
    fn set_control_value(&'a mut self, value: String) -> Result<Vec<&'a Variation>> {
        if let Some(v) = self.variations.first_mut() {
            v.variation.value = value;
        }
        Ok(self.variations())
    }
    fn set_variations(&'a mut self, variations: Vec<Variation>) -> Result<Vec<&'a Variation>> {
        let mut accumulated = Vec::<AccumulatedVar>::with_capacity(variations.len() + 1);
        let mut weight_sum: i16 = 0;

        for var in variations {
            let weight = var.weight;

            accumulated.push(AccumulatedVar {
                accum: weight,
                variation: Variation {
                    id: Some(var.id.unwrap_or_else(Ulid::new)),
                    value: var.value,
                    weight,
                },
            });
            weight_sum += weight;
        }
        if weight_sum > 100 {
            bail!("Environmental weights greater than 100%")
        }

        let mut control = self.variations.pop().unwrap();
        control.variation.weight = 100 - weight_sum;
        control.accum = 100 - weight_sum;
        accumulated.push(control);

        let _ = std::mem::replace(&mut self.variations, accumulated);
        Ok(self.variations())
    }

    fn variations(&self) -> Vec<&Variation> {
        self.variations.iter().map(|acc| &acc.variation).collect()
    }

    /// When `distribute` is being called:
    /// - choose the variation with the largest `accum`
    /// - subtract 100 from the `accum` for the chosen variation
    /// - add `weight` to `accum` for all variations, including the chosen one
    fn distribute(&mut self, _ident: Option<String>) -> &Variation {
        let max_accum = self
            .variations
            .iter_mut()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.accum.cmp(&b.accum));

        // there should be always at least one variation - a control value
        let (idx, var) = max_accum.unwrap();

        var.accum -= 100;

        for var in self.variations.iter_mut() {
            var.accum += var.variation.weight;
        }
        self.variations.get(idx).map(|v| &v.variation).unwrap()
    }
}
