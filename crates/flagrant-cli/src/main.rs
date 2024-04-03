use flagrant_client::blocking::HttpClient;
use repl::{context::HttpClientContext, readline};
use std::cell::RefCell;

mod handlers;
mod repl;

const API_HOST: &str = "http://localhost:3030";

fn main() -> anyhow::Result<()> {

    // todo: will be taken from args
    let project_id = 295;
    let environment = "development";

    let client = HttpClient::new(API_HOST.into(), project_id, environment.into());
    let context = RefCell::new(HttpClientContext::new(client)?);
    readline::init(context)?;

    Ok(())
}
