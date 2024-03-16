use flagrant_client::blocking::HttpClient;
use repl::{context::HttpClientContext, readline};
use std::{rc::Rc, sync::Mutex};

mod repl;
mod handlers;

const API_HOST: &str = "http://localhost:3030";

fn main() -> anyhow::Result<()> {
    let project_id = 295;
    let client = HttpClient::new(API_HOST.into(), project_id);

    let context = Rc::new(Mutex::new(HttpClientContext::new(client)?));
    readline::init(context)?;

    Ok(())
}
