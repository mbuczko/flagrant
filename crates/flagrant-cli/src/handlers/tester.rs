use anyhow::bail;
use flagrant_client::connection::Connection;
use flagrant_repl::{command::Arg, session::Session};
use flagrant_types::FeatureResponse;

use crate::printer::tabular::Tabular;

/// Splits a `[feature][@identity]` token on the *first* `@` - feature names never
/// contain `@`, while identities legitimately can (e.g. email-based identities like
/// `michal@buczko.pl`), so everything after the first `@` unambiguously belongs to
/// the identity. An empty side (bare `@michal`, trailing `ui_theme@`, or a lone `@`)
/// is treated as "not given", not an error.
fn split_feature_identity(arg: &str) -> (Option<&str>, Option<&str>) {
    let arg = arg.trim();
    if arg.is_empty() {
        return (None, None);
    }
    match arg.split_once('@') {
        Some((feature, identity)) => (
            (!feature.is_empty()).then_some(feature),
            (!identity.is_empty()).then_some(identity),
        ),
        None => (Some(arg), None),
    }
}

/// Expected args: `[feature][@identity]`
pub fn get(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if args.len() > 1 {
        bail!("Unexpected extra input. Usage: `GET [feature][@identity]`.");
    }
    let (feature_arg, identity_arg) = match args.first() {
        Some(arg) => split_feature_identity(arg),
        None => (None, None),
    };
    let ctx = session.context.read().unwrap();

    let feature_name = feature_arg
        .map(str::to_owned)
        .or_else(|| ctx.feature.as_ref().map(|f| f.name.clone()))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No feature given and none in context. Use `GET <feature>@<identity>` or set a feature context with \"/FEATURE <feature>\"."
            )
        })?;

    let identity_value = identity_arg
        .map(str::to_owned)
        .or_else(|| ctx.identity.as_ref().map(|i| i.value.clone()))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No identity given and none in context. Use `GET <feature>@<identity>` or set an identity context with \"/IDENTITY <identity>\"."
            )
        })?;

    let features = ctx.get_features(&identity_value)?;
    drop(ctx);

    match features.into_iter().find(|f| f.name == feature_name) {
        Some(f) => {
            f.display(None, &());
            Ok(())
        }
        None => bail!("Feature `{feature_name}` not found for identity `{identity_value}`."),
    }
}

/// Expected args: `[@identity]`
pub fn get_all(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if args.len() > 1 {
        bail!("Unexpected extra input. Usage: `GETALL [@identity]`.");
    }
    let identity_value = match args.first() {
        None => {
            let ctx = session.context.read().unwrap();
            ctx.identity
                .as_ref()
                .map(|i| i.value.clone())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "No identity given and none in context. Use `GETALL @<identity>` or set an identity context with \"/IDENTITY <identity>\"."
                    )
                })?
        }
        Some(arg) => arg
            .strip_prefix('@')
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow::anyhow!("Expected `@<identity>`, got `{arg}`."))?
            .to_owned(),
    };

    let ctx = session.context.read().unwrap();
    let features = ctx.get_features(&identity_value)?;
    drop(ctx);
    FeatureResponse::list(&features);
    Ok(())
}
