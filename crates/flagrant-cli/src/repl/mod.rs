use std::sync::{Arc, Mutex};
use flagrant_client::blocking::HttpClient;

pub mod command;
pub mod completer;
pub mod hinter;
pub mod readline;

pub type HttpClientContext = Arc<Mutex<HttpClient>>;
