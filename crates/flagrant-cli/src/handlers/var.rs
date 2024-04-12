use anyhow::bail;
use flagrant_types::{Feature, NewVariantRequestPayload, Variant};
use itertools::Itertools;

use crate::repl::session::{ReplSession, Resource};

pub fn add(args: Vec<&str>, session: &ReplSession) -> anyhow::Result<()> {
    if let Some((_, feature_name, weight, value)) = args.iter().collect_tuple() {
        let ssn = session.borrow();
        let res = ssn.environment.as_base_resource();

        if let Ok(mut feats) = ssn
            .client
            .get::<Vec<Feature>>(res.to_path(format!("/features?name={feature_name}")))
            && !feats.is_empty()
        {
            let feat = feats.remove(0);
            let variant = ssn.client.post::<_, Variant>(
                res.to_path(format!("/features/{}/variants", feat.id)),
                NewVariantRequestPayload {
                    value: value.to_string(),
                    weight: weight.parse::<u16>()?,
                },
            )?;

            println!("Created new variants for feature '{feature_name}' ({variant})");
            return Ok(());
        }
        bail!("Feature not found.")
    }
    bail!("No feature name, value or weight provided.")
}

pub fn list(args: Vec<&str>, session: &ReplSession) -> anyhow::Result<()> {
    if let Some(feature_name) = args.get(1) {
        let ssn = session.borrow();
        let res = ssn.environment.as_base_resource();

        if let Ok(mut feats) = ssn
            .client
            .get::<Vec<Feature>>(res.to_path(format!("/features?name={feature_name}")))
            && !feats.is_empty()
        {
            let feature = feats.remove(0);
            let variants: Vec<Variant> = ssn
                .client
                .get(res.to_path(format!("/features/{}/variants", feature.id)))?;

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
        bail!("Feature not found.")
    }
    bail!("No feature name provided.")
}

pub fn del(args: Vec<&str>, session: &ReplSession) -> anyhow::Result<()> {
    if let Some(variant_id) = args.get(1) {
        let ssn = session.borrow();
        let res = ssn.environment.as_base_resource();

        ssn.client
            .delete(res.to_path(format!("/variants/{variant_id}")))?;

        println!("Removed variant id={variant_id}");
        return Ok(());
    }
    bail!("No variant-id provided.")
}
