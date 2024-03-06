use anyhow::{anyhow, bail, Result};
use distributor::{AccumulativeDistributor, Distributor, Variation};
use semver::Version;
use sqlx::{Pool, Sqlite};
use ulid::Ulid;

pub mod db;
pub mod distributor;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FeatureValue<'a> {
    Simple(&'a String),
    Variadic(&'a Variation),
}

#[derive(Debug)]
pub struct Feature<'a> {
    name: String,
    value: String,
    is_enabled: bool,
    distributor: Option<Box<dyn Distributor<'a>>>,
}

impl<'a> Feature<'a> {
    pub fn new(name: String, value: String) -> Result<Self> {
        Ok(Feature {
            name,
            is_enabled: false,
            value,
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
            value: control_value,
            distributor: Some(Box::new(distributor)),
        })
    }

    /// Resets a feature value. In case when feature happens to be a variadic
    /// one, value also becomes a "control value".
    pub fn set_value(&mut self, control_value: String) -> Result<()> {
        self.value = control_value.clone();
        if let Some(distributor) = &mut self.distributor {
            distributor.set_control_value(control_value)?
        }
        Ok(())
    }

    /// Resets `is_enabled` feature state.
    pub fn toggle_enabled(&mut self, is_enabled: bool) -> &mut Self {
        self.is_enabled = is_enabled;
        self
    }

    /// Turns a possibly variadic feature into a simple one.
    pub fn simplify(&mut self) {
        self.distributor = None;
    }

    /// Returns a feature value which might be a simple String or
    /// a `Variation` in case when feature is a variadic one.
    ///
    /// For variadic features depending on `id` either known variation
    /// is returned, or this method call is being distributed in a way
    /// that depending on weights a matching variation is chosen and
    /// returned as result.
    pub fn value(&mut self, id: Option<Ulid>) -> Result<FeatureValue> {
        if self.distributor.is_some() {
            if let Some(id) = id {
                if let Some(variation) = self.variation(id)? {
                    return Ok(FeatureValue::Variadic(variation));
                }
                bail!("No feature variation of given id found.");
            }
            return Ok(FeatureValue::Variadic(
                self.distributor.as_mut().unwrap().distribute(),
            ));
        }
        Ok(FeatureValue::Simple(&self.value))
    }

    /// Returns a variation of given `id` if feature is variadic one.
    /// Bails out with error otherwise.
    pub fn variation(&mut self, id: Ulid) -> Result<Option<&Variation>> {
        Ok(self.variations()?.into_iter().find(|v| v.id == id))
    }

    /// Returns a vector of feature variations if feature is variadic one.
    /// Bails out with error otherwise.
    pub fn variations(&self) -> Result<Vec<&Variation>> {
        if let Some(distributor) = &self.distributor {
            Ok(distributor.variations())
        } else {
            Err(anyhow!("Not a variadic feature."))
        }
    }
}

pub async fn init() -> Result<Pool<Sqlite>> {
    let pool = db::init_pool()
        .await
        .expect("Could not connect to database");

    db::migrate(&pool, Version::parse(env!("CARGO_PKG_VERSION")).unwrap()).await?;
    Ok(pool)
}
