use anyhow::bail;

use crate::repl::context::ReplContext;

pub fn add(args: Vec<&str>, context: &ReplContext) -> anyhow::Result<()> {
    if args.is_empty() {
        bail!("Not enough parameters provided.");
    }
    println!("Adding new variant");
    Ok(())
}
