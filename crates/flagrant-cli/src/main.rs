use flagrant_client::blocking::HttpClient;
use repl::readline;
use std::sync::{Arc, Mutex};

mod repl;

fn main() -> anyhow::Result<()> {
    let project_id = 295;
    let client = HttpClient::new(project_id).expect("Project does not exist");

    let context = Arc::new(Mutex::new(client));
    readline::init(context)?;

    Ok(())
}
