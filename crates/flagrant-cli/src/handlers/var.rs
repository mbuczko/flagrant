use anyhow::bail;
use flagrant_types::{NewVariantRequestPayload, Variant};
use itertools::Itertools;

use crate::repl::context::ReplContext;

pub fn add(args: Vec<&str>, context: &ReplContext) -> anyhow::Result<()> {
    if let Some((_, feature_name, weight, value)) = args.iter().collect_tuple() {
        let env = &context.read().unwrap().environment;
        if env.is_none() {
            bail!("Environment not set. Use ENV sw <environment> to set it up.");
        }
        let var = context.read().unwrap().client.post::<_, _, Variant>(
            format!(
                "/variants/feature/{feature_name}/env/{}",
                env.as_ref().unwrap().name
            ),
            NewVariantRequestPayload {
                value: value.to_string(),
                weight: weight.parse::<u16>()?,
            },
        )?;

        println!(
            "Created new variants for feature '{feature_name}' (weight={}, value={})",
            var.weight, var.value
        );
        return Ok(())
    }
    bail!("No feature name, value or weight provided.")
}

pub fn list(args: Vec<&str>, context: &ReplContext) -> anyhow::Result<()> {
    if let Some(feature_name) = args.get(1) {
        let env = &context.read().unwrap().environment;
        if env.is_none() {
            bail!("Environment not set. Use ENV sw <environment> to set it up.");
        }
        let variants: Vec<Variant> = context.read().unwrap().client.get(format!(
            "/variants/feature/{feature_name}/env/{}",
            env.as_ref().unwrap().name
        ))?;

        println!("{:-^60}", "");
        println!("{0: <4} | {1: <10} | {2: <50}", "id", "weight", "value");
        println!("{:-^60}", "");

        for var in variants {
            println!(
                "{0: <4} | {1: <10} | {2: <50}",
                var.id, var.weight, var.value
            );
        }
        return Ok(());
    }
    bail!("No feature name provided.")
}

pub fn del(args: Vec<&str>, context: &ReplContext) -> anyhow::Result<()> {
    if let Some(variant_id) = args.get(1) {
        context
            .read()
            .unwrap()
            .client
            .delete(format!("/variants/{variant_id}"))?;

        println!("Removed variant id={variant_id}");
        return Ok(());
    }
    bail!("No variant-id provided.")
}
