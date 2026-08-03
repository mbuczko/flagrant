mod environment;
pub mod feature;
mod group;
mod identity;
mod rule;
pub mod segment;

pub trait Tabular {
    type Patch;
    type Context;

    fn display(&self, patch: Option<&Self::Patch>, ctx: &Self::Context);

    fn list(rows: &[Self])
    where
        Self: Sized;
}
