use anyhow::bail;
use flagrant_types::{NewVariantRequestPayload, Variant};

use crate::repl::context::ReplContext;

pub fn add(args: Vec<&str>, context: &ReplContext) -> anyhow::Result<()> {
    if args.is_empty() {
        bail!("Not enough parameters provided.");
    }
    if let Some(feature_name) = args.get(1) {
        if let (Some(weight), Some(value)) = (args.get(2), args.get(3)) {
            let env = &context.read().unwrap().environment;
            if env.is_none() {
                bail!("Environment not set. Use ENV sw <environment> to set it up.");
            }
            let payload = NewVariantRequestPayload {
                value: value.to_string(),
                weight: weight.parse::<u16>()?,
            };
            let var: Variant = context.read().unwrap().client.post(
                format!("/features/{feature_name}/{}/variants", env.as_ref().unwrap().name),
                payload,
            )?;

            println!(
                "Created new variants for feature '{feature_name}' (weight={}, value={})",
                var.weight, var.value
            );
            return Ok(());
        }
        bail!("Variant weight or value not provided")
    }
    bail!("No feature name or value provided")
}
