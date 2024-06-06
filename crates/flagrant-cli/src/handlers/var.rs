use anyhow::bail;
use ascii_table::AsciiTable;
use flagrant_client::session::{Resource, Session};
use flagrant_types::{Feature, Tabular, Variant, VariantRequestPayload};

use crate::repl::readline::ReplEditor;

pub fn add(args: &[&str], session: &Session, _: &mut ReplEditor) -> anyhow::Result<()> {
    if let Some(feature_name) = args.get(1) {
        let res = session.environment.as_base_resource();

        if let Ok(feat) = session
            .client
            .get::<Feature>(res.subpath(format!("/features/name/{feature_name}")))
        {
            let variant = match (args.get(2), args.get(3)) {
                (Some(&weight), Some(&value)) => session.client.post::<_, Variant>(
                    res.subpath(format!("/features/{}/variants", feat.id)),
                    VariantRequestPayload {
                        value: value.to_string(),
                        weight: weight.parse::<i16>()?,
                    },
                )?,
                _ => bail!("No weight or value provided."),
            };

            variant.tabular_print();
            return Ok(());
        }
        bail!("Feature not found.")
    }
    bail!("No feature name provided.")
}

pub fn list(args: &[&str], session: &Session, _: &mut ReplEditor) -> anyhow::Result<()> {
    if let Some(feature_name) = args.get(1) {
        let res = session.environment.as_base_resource();
        let response = session
            .client
            .get::<Feature>(res.subpath(format!("/features/name/{feature_name}")));

        match response {
            Ok(feature) => {
                let variants: Vec<Variant> = session
                    .client
                    .get(res.subpath(format!("/features/{}/variants", feature.id)))?;

                let mut table = AsciiTable::default();
                let mut vecs = Vec::with_capacity(variants.len() + 1);

                table.column(0).set_header("ID");
                table.column(1).set_header("WEIGHT");
                table.column(2).set_header("VALUE");

                for var in variants {
                    vecs.push(vec![
                        var.id.to_string(),
                        bar(var.weight, 10),
                        var.value.trim().to_string(),
                    ])
                }
                table.print(vecs);
                return Ok(());
            }
            Err(error) => {
                bail!(error)
            }
        }
    }
    bail!("No feature name provided.")
}

pub fn del(args: &[&str], session: &Session, _: &mut ReplEditor) -> anyhow::Result<()> {
    if let Some(variant_id) = args.get(1) {
        let res = session.environment.as_base_resource();

        session
            .client
            .delete(res.subpath(format!("/variants/{variant_id}")))?;

        println!("Removed variant id={variant_id}");
        return Ok(());
    }
    bail!("No variant-id provided.")
}

pub fn update(args: &[&str], session: &Session, _: &mut ReplEditor) -> anyhow::Result<()> {
    if let Some(variant_id) = args.get(1) {
        let res = session.environment.as_base_resource();

        if let Ok(variant) = session
            .client
            .get::<Variant>(res.subpath(format!("/variants/{variant_id}")))
        {
            let (weight, value) = match (args.get(2), args.get(3)) {
                (Some(&weight), Some(&value)) => (weight, value.to_string()),
                (Some(&weight), _) => (weight, variant.value),
                _ => bail!("No weight provided."),
            };
            let weight = weight.parse::<i16>()?;
            if weight < 0 {
                bail!("Variant weight should be positive number in range of <0, 100>.")
            }
            session.client.put(
                res.subpath(format!("/variants/{}", variant.id)),
                VariantRequestPayload { value, weight },
            )?;

            // re-fetch variant to be sure it's updated
            let variant = session
                .client
                .get::<Variant>(res.subpath(format!("/variants/{variant_id}")))?;

            variant.tabular_print();
            return Ok(());
        }
        bail!("No variant of given id found.");
    }
    bail!("No variant-id provided.")
}

fn bar(weight: i16, width: i16) -> String {
    let mut bar = vec![' '; width as usize];
    let progress = weight * width / 100;

    for ch in bar.iter_mut().take(progress as usize) {
        *ch = '▆';
    }
    format!("{0: <3}% {1: <10}", weight, String::from_iter(bar))
}
