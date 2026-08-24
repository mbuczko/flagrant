use anyhow::bail;
use flagrant_client::connection::Connection;
use flagrant_repl::{command::Arg, session::Session};
use serde_json::Value;

/// Asks the server to reload its configuration file from disk. Takes no arguments.
pub fn reload(_args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let ctx = session.context.read().unwrap();
    match ctx
        .client
        .post::<_, Value>("/admin/reload".into(), Value::Null)
    {
        Ok(_) => {
            println!("Server configuration reloaded.");
            Ok(())
        }
        Err(err) => bail!("Could not reload server configuration: {err}"),
    }
}
