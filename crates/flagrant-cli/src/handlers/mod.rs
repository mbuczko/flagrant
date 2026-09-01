pub mod admin;
pub mod context;
pub mod environments;
pub mod features;
pub mod groups;
pub mod identities;
pub mod projects;
pub mod rules;
pub mod segments;
pub mod snapshots;
pub mod tester;
pub mod variants;

pub(crate) mod internal;

pub(crate) use internal::prompt_line;
pub(crate) use internal::stage::{commit, discard, reset};
