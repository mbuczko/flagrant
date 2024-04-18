use anyhow::bail;
use flagrant_types::{Feature, VariantRequestPayload, Variant};
use itertools::Itertools;

use crate::repl::session::{ReplSession, Resource};

pub fn add(args: &[&str], session: &ReplSession) -> anyhow::Result<()> {
    if let Some((_, feature_name, weight, value)) = args.iter().take(4).collect_tuple() {
        let ssn = session.borrow();
        let res = ssn.environment.as_base_resource();

        if let Ok(mut feats) = ssn
            .client
            .get::<Vec<Feature>>(res.subpath(format!("/features?name={feature_name}")))
            && !feats.is_empty()
        {
            let feat = feats.remove(0);
            let variant = ssn.client.post::<_, Variant>(
                res.subpath(format!("/features/{}/variants", feat.id)),
                VariantRequestPayload {
                    value: value.to_string(),
                    weight: weight.parse::<i16>()?,
                },
            )?;

            println!("{variant}");
            return Ok(());
        }
        bail!("Feature not found.")
    }
    bail!("No feature name, value or weight provided.")
}

pub fn list(args: &[&str], session: &ReplSession) -> anyhow::Result<()> {
    if let Some(feature_name) = args.get(1) {
        let ssn = session.borrow();
        let res = ssn.environment.as_base_resource();

        if let Ok(mut feats) = ssn
            .client
            .get::<Vec<Feature>>(res.subpath(format!("/features?name={feature_name}")))
            && !feats.is_empty()
        {
            let feature = feats.remove(0);
            let variants: Vec<Variant> = ssn
                .client
                .get(res.subpath(format!("/features/{}/variants", feature.id)))?;

            println!("{:─^60}", "");
            println!("{0: <4} │ {1: <15} │ {2: <50}", "ID", "WEIGHT", "VALUE");
            println!("{:─^60}", "");

            for var in variants {
                println!(
                    "{0: <4} │ {1: <14} │ {2: <50}",
                    var.id,
                    bar(var.weight, 10),
                    var.value
                );
            }
            return Ok(());
        }
        bail!("Feature not found.")
    }
    bail!("No feature name provided.")
}

pub fn del(args: &[&str], session: &ReplSession) -> anyhow::Result<()> {
    if let Some(variant_id) = args.get(1) {
        let ssn = session.borrow();
        let res = ssn.environment.as_base_resource();

        ssn.client
            .delete(res.subpath(format!("/variants/{variant_id}")))?;

        println!("Removed variant id={variant_id}");
        return Ok(());
    }
    bail!("No variant-id provided.")
}

pub fn weight(args: &[&str], session: &ReplSession) -> anyhow::Result<()> {
    if let Some((_, variant_id, weight)) = args.iter().take(3).collect_tuple() {
        let ssn = session.borrow();
        let res = ssn.environment.as_base_resource();

        if let Ok(variant) = ssn
            .client
            .get::<Variant>(res.subpath(format!("/variants/{variant_id}")))
        {
            let weight = weight.parse::<i16>()?;
            if weight < 0 {
                bail!("Variant weight should be positive number in range of <0, 100>.")
            }
            ssn.client.put(
                res.subpath(format!("/variants/{}", variant.id)),
                VariantRequestPayload {
                    value: variant.value,
                    weight
                },
            )?;
            return Ok(());
        }
        bail!("No variant of given id found.");
    }
    bail!("No variant-id or new weight provided.")
}

pub fn value(args: &[&str], session: &ReplSession) -> anyhow::Result<()> {
    if let Some((_, variant_id, value)) = args.iter().take(3).collect_tuple() {
        let ssn = session.borrow();
        let res = ssn.environment.as_base_resource();

        if let Ok(variant) = ssn.client.get::<Variant>(res.subpath(format!("/variants/{variant_id}"))) {
            ssn.client.put(
                res.subpath(format!("/variants/{}", variant.id)),
                VariantRequestPayload {
                    value: value.to_string(),
                    weight: variant.weight
                },
            )?;

            // re-fetch variant to be sure it's updated
            let variant = ssn.client.get::<Variant>(res.subpath(format!("/variants/{variant_id}")))?;
            println!("{variant}");

            return Ok(());
        }
        bail!("No variant of given id found.");
    }
    bail!("No variant-id or new value provided.")
}

fn bar(weight: i16, width: i16) -> String {
    let mut bar = vec![' '; width as usize];
    let progress = weight * width / 100;

    for ch in bar.iter_mut().take(progress as usize) {
        *ch = '▆';
    }
    format!("{0: <3}% {1: <10}", weight, String::from_iter(bar))
}
