//! Internal helpers shared across variant handlers.
//!
//! - [`index`]: maintains the positional variant index and resolves display numbers to [`VariantRef`] values.
//! - [`stage`]: upserts and discards ops in the pending [`FeaturePatch`] before it is committed.
//! - [`variants`]: computes the effective (committed + staged) variant list.

pub(crate) mod helpers;
pub(crate) mod index;
pub(crate) mod stage;
pub(crate) mod variants;

pub(crate) use helpers::open_in_editor;
