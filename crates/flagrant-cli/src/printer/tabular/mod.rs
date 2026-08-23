pub(crate) mod environment;
pub mod feature;
mod feature_response;
mod group;
mod identity;
mod rollout;
mod rule;
pub mod segment;
mod snapshot;
mod variant;

pub trait Tabular {
    type Patch;
    type Context;

    fn display(&self, patch: Option<&Self::Patch>, ctx: &Self::Context);

    fn list(rows: &[Self])
    where
        Self: Sized;
}
