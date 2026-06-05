pub mod environments;
pub mod features;
pub mod identities;
pub mod projects;
pub mod variants;

pub(crate) mod internal;

pub(crate) use internal::stage::{commit, discard};
pub(crate) use internal::open_in_editor;
