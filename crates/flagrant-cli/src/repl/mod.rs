use std::sync::{Arc, Mutex};

use self::context::HttpClientContext;

pub mod command;
pub mod completer;
pub mod hinter;
pub mod readline;
pub mod context;

pub type ReplContext = Arc<Mutex<HttpClientContext>>;
