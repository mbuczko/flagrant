use flagrant_client::blocking::HttpClient;
use repl::{context::HttpClientContext, readline};
use std::{rc::Rc, sync::RwLock};

mod handlers;
mod repl;

const API_HOST: &str = "http://localhost:3030";

fn main() -> anyhow::Result<()> {
    let project_id = 295;
    let environment = "development";
    let client = HttpClient::new(API_HOST.into(), project_id, environment.into());

    let context = Rc::new(RwLock::new(HttpClientContext::new(client)?));
    readline::init(context)?;

    Ok(())
}
