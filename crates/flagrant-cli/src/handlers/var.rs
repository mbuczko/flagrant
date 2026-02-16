use std::borrow::Cow;

use anyhow::bail;
use flagrant_client::connection::{Connection, Resource};
use flagrant_repl::{command::Arg, session::Session};
use flagrant_types::{Feature, Variant, payload::VariantRequestPayload};

use crate::printer::tabular::Tabular;

pub fn add(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(feature_name) = args.get(1) {
        let ctx = session.context.read().unwrap();
        let res = ctx.environment.as_base_resource();
        let val = args
            .get(3)
            .map(|a| Cow::from(a.to_string()))
            .unwrap_or(Cow::Owned(String::default()));

        let response = ctx
            .client
            .get::<Vec<Feature>>(res.subpath(format!("/features/{feature_name}")));

        if let Ok(mut features) = response
            && let Some(feature) = features.pop()
        {
            // Take default variant's value type and use it to construct value for
            // variant being created, according to following rules:
            //
            // - if given value hasn't been explicitly typed (like json::{"a": 2}) use default
            //   variant's value type
            // - if value has been explicitly typed priotitize type over default variant's type

            let cloned = val
                .parse()
                .unwrap_or_else(|_| feature.get_default_value().clone_with(&val));

            let weight = match args.get(2) {
                Some(weight) => weight.parse::<u8>()?,
                _ => bail!("No weight or value provided."),
            };

            if !(0..=100).contains(&weight) {
                bail!("Variant weight should be positive number in range of <0, 100>.")
            }

            let variant = ctx.client.post::<_, Variant>(
                res.subpath(format!("/features/{}/variants", feature.id)),
                VariantRequestPayload {
                    value: cloned.to_string(),
                    weight,
                },
            )?;
            variant.describe();
            return Ok(());
        }
        bail!("Feature not found.")
    }
    bail!("No feature name provided.")
}

pub fn value(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(variant_id) = args.get(1) {
        let ctx = session.context.read().unwrap();
        let res = ctx.environment.as_base_resource();
        let val = args
            .get(2)
            .map(|a| Cow::from(a.to_string()))
            .unwrap_or(Cow::Owned(String::default()));

        let response = ctx
            .client
            .get::<Variant>(res.subpath(format!("/variants/{variant_id}")));

        if let Ok(variant) = response {
            // update variant value according to following rules:
            // - if given value hasn't been explicitly typed (like json::{"a": 2}) use current
            //   variant's value type
            // - if value has been explicitly typed priotitize type over current variant's type

            let cloned = val
                .parse()
                .unwrap_or_else(|_| variant.value.clone_with(&val));

            ctx.client.put(
                res.subpath(format!("/variants/{}", variant.id)),
                VariantRequestPayload {
                    value: cloned.to_string(),
                    weight: variant.weight,
                },
            )?;
            // re-fetch variant to be sure it's updated
            let variant = ctx
                .client
                .get::<Variant>(res.subpath(format!("/variants/{variant_id}")))?;

            variant.describe();
            return Ok(());
        }
        bail!("No variant of given id found.");
    }
    bail!("No variant-id provided.")
}

pub fn weight(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(variant_id) = args.get(1) {
        let ctx = session.context.read().unwrap();
        let res = ctx.environment.as_base_resource();
        let response = ctx
            .client
            .get::<Variant>(res.subpath(format!("/variants/{variant_id}")));

        if let Ok(variant) = response {
            if let Some(weight) = args.get(2) {
                let weight = weight.parse::<u8>()?;

                if !(0..=100).contains(&weight) {
                    bail!("Variant weight should be positive number in range of <0, 100>.")
                }
                ctx.client.put(
                    res.subpath(format!("/variants/{}", variant.id)),
                    VariantRequestPayload {
                        value: variant.value.to_string(),
                        weight,
                    },
                )?;
                // re-fetch variant to be sure it's updated
                let variant = ctx
                    .client
                    .get::<Variant>(res.subpath(format!("/variants/{variant_id}")))?;

                variant.describe();
                return Ok(());
            }
            bail!("No variant weight provided.");
        }
        bail!("No variant of given id found.");
    }
    bail!("No variant-id provided.")
}

pub fn list(_args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let ctx = session.context.read().unwrap();

    if let Some(feature) = ctx.feature.as_ref() {
        Variant::list(&feature.variants);
        return Ok(());
    }
    bail!("No feature name provided.")
}

pub fn del(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(variant_id) = args.get(1) {
        let ctx = session.context.read().unwrap();
        let res = ctx.environment.as_base_resource();

        ctx.client
            .delete(res.subpath(format!("/variants/{variant_id}")))?;

        println!("Removed variant id={variant_id}");
        return Ok(());
    }
    bail!("No variant-id provided.")
}
