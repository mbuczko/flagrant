#![feature(let_chains)]

use flagrant_client::blocking::HttpClient;
use repl::{session::Session, readline};

mod handlers;
mod repl;

const API_HOST: &str = "http://localhost:3030";

fn main() -> anyhow::Result<()> {
    // todo: will be taken from args
    let project_id = 1;
    let environment_id = 1;

    let client = HttpClient::new(API_HOST.into());
    let session = Session::init(
        client,
        project_id,
        environment_id,
    )?;
    readline::init(session)?;

    Ok(())
}
