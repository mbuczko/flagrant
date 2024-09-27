use anyhow::bail;
use fancy_table::{Align, FancyTable, FancyTableOpts, Layout};
use flagrant_client::session::{Session, Resource};
use flagrant_types::{payloads::EnvRequestPayload, Environment};

use crate::{repl::readline::ReplEditor, tabular::Tabular};

/// Adds a new Environment
pub fn add(args: &[&str], session: &Session, _: &mut ReplEditor) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let res = session.project.as_base_resource();
        let env = session.client.post::<_, Environment>(
            res.subpath("/envs"),
            EnvRequestPayload {
                name: name.to_string(),
                description: args.get(2).map(|d| d.to_string()),
            },
        )?;

        env.tabular_print();
        return Ok(());
    }
    bail!("No environment name provided.")
}

/// Lists all environments
pub fn list(_args: &[&str], session: &Session, _: &mut ReplEditor) -> anyhow::Result<()> {
    let res = session.project.as_base_resource();
    let envs = session
        .client
        .get::<Vec<Environment>>(res.subpath("/envs"))?;

    let table = FancyTable::create(FancyTableOpts::default())
        .add_column_named_with_align("ID".into(), Layout::Fixed(6), Align::Left)
        .add_column_named_with_align("NAME".into(), Layout::Expandable(50), Align::Left)
        .add_column_named_with_align("DESCRIPTION".into(), Layout::Expandable(100), Align::Left)
        .rseparator(None)
        .build(80);

    let mut rows = Vec::with_capacity(envs.len());
    for env in envs {
        rows.push(vec![
            env.id.to_string(),
            env.name,
            env.description.unwrap_or_default(),
        ]);
    }

    table.render(rows);
    Ok(())
}

/// Changes current environment in a session
pub fn switch(args: &[&str], session: &Session, _: &mut ReplEditor) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        let res = session.project.as_base_resource();
        let response = session
            .client
            .get::<Environment>(res.subpath(format!("/envs/name/{name}")));

        if let Ok(env) = response {
            println!("Switching to environment '{}' (id={})", env.name, env.id);
            session.set_environment(env);
            return Ok(());
        }
        bail!("No such an environment.")
    }
    bail!("No environment name provided.");
}
