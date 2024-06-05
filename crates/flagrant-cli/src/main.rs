#![feature(let_chains)]

use flagrant_client::session::Session;
use repl::readline;

mod handlers;
mod repl;

const API_HOST: &str = "http://localhost:3030";

fn main() -> anyhow::Result<()> {
    // todo: will be taken from args
    let project_id = 1;
    let environment_id = 1;

    let session = Session::init(
        API_HOST.into(),
        project_id,
        environment_id,
    )?;
    readline::init(session)?;

    Ok(())
}
