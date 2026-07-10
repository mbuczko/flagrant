mod environment;
pub mod feature;
mod identity;
mod segment;

pub trait Tabular {
    type Patch;
    type Context;

    fn describe(&self, patch: Option<&Self::Patch>, ctx: &Self::Context);

    fn list(rows: &[Self])
    where
        Self: Sized;
}
