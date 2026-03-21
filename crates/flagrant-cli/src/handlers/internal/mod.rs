//! Internal helpers shared across variant handlers.
//!
//! - [`index`]: maintains the positional variant index and resolves display numbers to [`VariantRef`] values.
//! - [`stage`]: upserts and discards ops in the pending [`FeaturePatch`] before it is committed.

pub(crate) mod index;
pub(crate) mod stage;
