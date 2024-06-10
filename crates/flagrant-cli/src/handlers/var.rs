use anyhow::bail;
use ascii_table::AsciiTable;
use flagrant_client::session::{Resource, Session};
use flagrant_types::{
    payloads::VariantRequestPayload, tabular::Tabular, Feature, FeatureValue, Variant,
};

use crate::repl::{multiline::multiline_value, readline::ReplEditor};

pub fn add(args: &[&str], session: &Session, editor: &mut ReplEditor) -> anyhow::Result<()> {
    if let Some(feature_name) = args.get(1) {
        let res = session.environment.as_base_resource();
        let val = args
            .get(3)
            .map(|v| v.to_string())
            .unwrap_or_else(|| multiline_value(editor).unwrap());

        if let Ok(feature) = session
            .client
            .get::<Feature>(res.subpath(format!("/features/name/{feature_name}")))
        {
            // take default variant's value type and use it to construct value for
            // variant being created, according to following rules:
            //
            // - if given value hasn't been explicitly typed (like json::{"a": 2}) use default
            //   variant's value type
            // - if value has been explicitly typed priotitize type over default variant's type

            let cloned = val.parse().unwrap_or_else(|_| {
                feature
                    .get_default_value()
                    .map(|v| v.clone_with(&val))
                    .unwrap_or_else(|| FeatureValue::Text(val))
            });
            let weight = match args.get(2) {
                Some(&weight) => weight.parse::<i16>()?,
                _ => bail!("No weight or value provided."),
            };

            if !(0..=100).contains(&weight) {
                bail!("Variant weight should be positive number in range of <0, 100>.")
            }

            let variant = session.client.post::<_, Variant>(
                res.subpath(format!("/features/{}/variants", feature.id)),
                VariantRequestPayload {
                    value: cloned.to_string(),
                    weight,
                },
            )?;
            variant.tabular_print();
            return Ok(());
        }
        bail!("Feature not found.")
    }
    bail!("No feature name provided.")
}

pub fn value(args: &[&str], session: &Session, editor: &mut ReplEditor) -> anyhow::Result<()> {
    if let Some(variant_id) = args.get(1) {
        let res = session.environment.as_base_resource();
        let val = args
            .get(2)
            .map(|v| v.to_string())
            .unwrap_or_else(|| multiline_value(editor).unwrap());

        if let Ok(variant) = session
            .client
            .get::<Variant>(res.subpath(format!("/variants/{variant_id}")))
        {
            // update variant value according to following rules:
            // - if given value hasn't been explicitly typed (like json::{"a": 2}) use  current
            //   variant's value type
            // - if value has been explicitly typed priotitize type over current variant's type

            let cloned = val
                .parse()
                .unwrap_or_else(|_| variant.value.clone_with(&val));

            session.client.put(
                res.subpath(format!("/variants/{}", variant.id)),
                VariantRequestPayload {
                    value: cloned.to_string(),
                    weight: variant.weight,
                },
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

pub fn weight(args: &[&str], session: &Session, _: &mut ReplEditor) -> anyhow::Result<()> {
    if let Some(variant_id) = args.get(1) {
        let res = session.environment.as_base_resource();

        if let Ok(variant) = session
            .client
            .get::<Variant>(res.subpath(format!("/variants/{variant_id}")))
        {
            if let Some(weight) = args.get(2) {
                let weight = weight.parse::<i16>()?;

                if !(0..=100).contains(&weight) {
                    bail!("Variant weight should be positive number in range of <0, 100>.")
                }
                session.client.put(
                    res.subpath(format!("/variants/{}", variant.id)),
                    VariantRequestPayload {
                        value: variant.value.to_string(),
                        weight,
                    },
                )?;
                // re-fetch variant to be sure it's updated
                let variant = session
                    .client
                    .get::<Variant>(res.subpath(format!("/variants/{variant_id}")))?;

                variant.tabular_print();
                return Ok(());
            }
            bail!("No variant weight provided.");
        }
        bail!("No variant of given id found.");
    }
    bail!("No variant-id provided.")
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
                        var.value.to_string(),
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

fn bar(weight: i16, width: i16) -> String {
    let mut bar = vec![' '; width as usize];
    let progress = weight * width / 100;

    for ch in bar.iter_mut().take(progress as usize) {
        *ch = '▆';
    }
    format!("{0: <3}% {1: <10}", weight, String::from_iter(bar))
}
