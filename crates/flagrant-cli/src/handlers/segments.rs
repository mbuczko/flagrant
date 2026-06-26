use anyhow::bail;
use flagrant_client::connection::Connection;
use flagrant_repl::{command::Arg, session::Session};
use flagrant_types::{
    Segment,
    payload::{NewSegmentPayload, SegmentPatchOp},
};

use crate::printer::tabular::Tabular;

fn fetch_segment(name: &str, session: &Session<Connection>) -> anyhow::Result<Segment> {
    let ctx = session.context.read().unwrap();
    let res = ctx.project_resource();

    ctx.client
        .get::<Segment>(res.subpath(format!("/segments/{name}")))
}

/// Create a new segment and enter its context.
///
/// Expected args: `<name> [description]`
pub fn add(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    {
        let ctx = session.context.read().unwrap();
        if ctx.has_segment_pending() {
            bail!("You have uncommitted segment changes. Run `COMMIT` or `DISCARD` first.");
        }
    }
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

/// List all segments in the current project, optionally filtered by a name substring.
///
/// An optional bare string argument is treated as a substring filter (e.g. `SEGMENT list seg`).
pub fn list(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let ctx = session.context.read().unwrap();
    let res = ctx.project_resource();
    let pat = args.get(1).map(std::ops::Deref::deref).unwrap_or("");

    Segment::list(
        ctx.client
            .get::<Vec<Segment>>(res.subpath(format!("/segments?pattern={pat}")))?
            .as_ref(),
    );
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
            segment.describe(ctx.segment_patch.as_ref().filter(|p| !p.is_empty()), &());
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
        ctx.client
            .delete(res.subpath(format!("/segments/{}", segment.id)))?;
    }

    let mut ctx = session.context.write().unwrap();
    if ctx.segment.as_ref().map(|s| s.id) == Some(segment.id) {
        ctx.segment = None;
    }
    println!("Segment '{}' deleted.", name);
    Ok(())
}

/// Stage a segment name change.
///
/// Expected args: `<name>`
pub fn set_name(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let name = args
        .get(1)
        .ok_or_else(|| anyhow::anyhow!("Usage: SET name <name>"))?;
    let mut ctx = session.context.write().unwrap();
    if ctx.segment.is_none() {
        bail!("Not in a segment context.");
    }
    ctx.get_or_init_segment_patch()
        .ops
        .push(SegmentPatchOp::SetName(name.to_string()));
    println!("Staged: name = {name}");
    Ok(())
}

/// Stage a segment description change.
///
/// Expected args: `[description]` (omit to clear)
pub fn set_description(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let desc = args.get(1).map(|a| a.to_string());
    let mut ctx = session.context.write().unwrap();
    if ctx.segment.is_none() {
        bail!("Not in a segment context.");
    }
    ctx.get_or_init_segment_patch()
        .ops
        .push(SegmentPatchOp::SetDescription(desc.clone()));
    println!(
        "Staged: description = {}",
        desc.as_deref().unwrap_or("(cleared)")
    );
    Ok(())
}

/// Commit all staged segment changes to the API.
pub fn commit(_args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();
    let segment_id = ctx
        .segment
        .as_ref()
        .map(|s| s.id)
        .ok_or_else(|| anyhow::anyhow!("Not in a segment context."))?;

    let patch = match &ctx.segment_patch {
        Some(p) if !p.is_empty() => p.clone(),
        _ => return Ok(()),
    };

    let path = ctx
        .project_resource()
        .subpath(format!("/segments/{segment_id}"));
    let updated = ctx
        .client
        .patch::<_, Segment>(path, patch)
        .map_err(|err| anyhow::anyhow!("Segment commit failed: {err}"))?;

    updated.describe(None, &());
    ctx.segment_patch = None;
    ctx.segment = Some(updated);
    Ok(())
}

/// Drop all staged segment changes.
pub fn discard(_args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let mut ctx = session.context.write().unwrap();
    if ctx.has_segment_pending() {
        ctx.discard_segment_patch();
        println!("Pending changes discarded.");
    }
    Ok(())
}

/// Enter segment context by name. Clears any active identity context.
///
/// Expected args: `<name>`
pub fn r#use(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    {
        let ctx = session.context.read().unwrap();
        if ctx.has_segment_pending() {
            bail!("You have uncommitted segment changes. Run `COMMIT` or `DISCARD` first.");
        }
    }
    let Some(name) = args.get(1) else {
        bail!("No segment name provided.");
    };
    let segment = fetch_segment(name, session)?;
    segment.describe(None, &());

    let mut ctx: std::sync::RwLockWriteGuard<'_, Connection> = session.context.write().unwrap();
    ctx.identity = None;
    ctx.identity_patch = None;
    ctx.segment = Some(segment);

    Ok(())
}
