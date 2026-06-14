use anyhow::bail;
use flagrant_client::connection::Connection;
use flagrant_repl::{command::Arg, session::Session};
use flagrant_types::{Segment, payload::NewSegmentPayload};

use crate::printer::tabular::Tabular;

fn fetch_segment(name: &str, session: &Session<Connection>) -> anyhow::Result<Segment> {
    let ctx = session.context.read().unwrap();
    let res = ctx.project_resource();
    ctx.client.get::<Segment>(res.subpath(format!("/segments/{name}")))
}

/// Create a new segment and enter its context.
///
/// Expected args: `<name> [description]`
pub fn add(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let Some(name) = args.get(1) else {
        bail!("No segment name provided.");
    };
    let segment = {
        let ctx = session.context.read().unwrap();
        let res = ctx.project_resource();
        ctx.client.post::<_, Segment>(
            res.subpath("/segments"),
            NewSegmentPayload {
                name: name.to_string(),
                description: args.get(2).map(|d| d.to_string()),
            },
        )?
    };
    segment.describe(None, &());
    session.context.write().unwrap().segment = Some(segment);
    Ok(())
}

/// List all segments in the current project.
pub fn list(_args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let ctx = session.context.read().unwrap();
    let res = ctx.project_resource();
    let segments = ctx.client.get::<Vec<Segment>>(res.subpath("/segments"))?;
    Segment::list(&segments);
    Ok(())
}

/// Describe a segment by name, or the current segment context.
///
/// Expected args: `[name]`
pub fn describe(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    if let Some(name) = args.get(1) {
        fetch_segment(name, session)?.describe(None, &());
    } else {
        let ctx = session.context.read().unwrap();
        if let Some(segment) = &ctx.segment {
            segment.describe(None, &());
        } else {
            bail!("Not in a segment context. Use `SEGMENT use <name>` first.");
        }
    }
    Ok(())
}

/// Delete a segment by name.
///
/// Expected args: `<name>`
pub fn delete(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let Some(name) = args.get(1) else {
        bail!("No segment name provided.");
    };
    let segment = fetch_segment(name, session)?;
    {
        let ctx = session.context.read().unwrap();
        let res = ctx.project_resource();
        ctx.client.delete(res.subpath(format!("/segments/{}", segment.id)))?;
    }
    let mut ctx = session.context.write().unwrap();
    if ctx.segment.as_ref().map(|s| s.id) == Some(segment.id) {
        ctx.segment = None;
    }
    println!("Segment '{}' deleted.", name);
    Ok(())
}

/// Enter segment context by name. Clears any active identity context.
///
/// Expected args: `<name>`
pub fn r#use(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let Some(name) = args.get(1) else {
        bail!("No segment name provided.");
    };
    let segment = fetch_segment(name, session)?;
    segment.describe(None, &());
    let mut ctx = session.context.write().unwrap();
    ctx.identity = None;
    ctx.identity_patch = None;
    ctx.segment = Some(segment);
    Ok(())
}
