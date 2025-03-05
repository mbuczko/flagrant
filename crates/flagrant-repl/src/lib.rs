#![feature(let_chains)]

use session::Session;

pub mod command;
pub mod completer;
pub mod hinter;
pub mod parser;
pub mod readline;
pub mod session;

type PromptFn<T> = fn(session: &Session<T>) -> String;
