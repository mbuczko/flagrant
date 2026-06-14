use anyhow::bail;
use flagrant_client::connection::Connection;
use flagrant_repl::{command::Arg, session::Session};
use flagrant_types::{GroupConnector, Segment, SegmentGroup, payload::NewGroupPayload};

use crate::printer::tabular::Tabular;

fn segment_from_ctx(session: &Session<Connection>) -> anyhow::Result<Segment> {
    let ctx = session.context.read().unwrap();
    ctx.segment
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Not in a segment context. Use `SEGMENT use <name>` first."))
}

fn refresh_segment(session: &Session<Connection>) -> anyhow::Result<Segment> {
    let segment_id = session
        .context
        .read()
        .unwrap()
        .segment
        .as_ref()
        .map(|s| s.id)
        .ok_or_else(|| anyhow::anyhow!("No segment in context."))?;
    let ctx = session.context.read().unwrap();
    let res = ctx.project_resource();
    ctx.client
        .get::<Segment>(res.subpath(format!("/segments/{segment_id}")))
}

/// Add a group to the current segment.
///
/// Expected args: `[--and|--and-not] [description]`
///
/// First group needs no flag (connector will be ignored). Subsequent groups require
/// `--and` or `--and-not`.
pub fn add(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let segment = segment_from_ctx(session)?;

    let (connector, description) = match args.get(1).map(|a| a.as_ref()) {
        Some("--and") => (Some(GroupConnector::And), args.get(2).map(|d| d.to_string())),
        Some("--and-not") => (
            Some(GroupConnector::AndNot),
            args.get(2).map(|d| d.to_string()),
        ),
        other => (None, other.map(str::to_string)),
    };

    let group = {
        let ctx = session.context.read().unwrap();
        let res = ctx.project_resource();
        ctx.client.post::<_, SegmentGroup>(
            res.subpath(format!("/segments/{}/groups", segment.id)),
            NewGroupPayload {
                description,
                connector,
            },
        )?
    };
    println!(
        "Added [{}]{}",
        group.label,
        group
            .description
            .as_deref()
            .map(|d| format!(" — {d}"))
            .unwrap_or_default()
    );

    // Refresh segment in context.
    let updated = refresh_segment(session)?;
    session.context.write().unwrap().segment = Some(updated);
    Ok(())
}

/// List groups of the current segment.
pub fn list(_args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let segment = segment_from_ctx(session)?;
    // Re-fetch to get latest state.
    let ctx = session.context.read().unwrap();
    let res = ctx.project_resource();
    let fresh = ctx
        .client
        .get::<Segment>(res.subpath(format!("/segments/{}", segment.id)))?;
    fresh.describe(None, &());
    Ok(())
}

/// Delete a group from the current segment by label.
///
/// Expected args: `<label>` (e.g. "group-1")
pub fn delete(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let segment = segment_from_ctx(session)?;
    let Some(label) = args.get(1) else {
        bail!("No group label provided. Expected: GROUP delete <label> (e.g. group-1)");
    };

    let group = segment
        .groups
        .iter()
        .find(|g| g.label == label.as_ref())
        .ok_or_else(|| anyhow::anyhow!("Group '{label}' not found in current segment."))?;

    {
        let ctx = session.context.read().unwrap();
        let res = ctx.project_resource();
        ctx.client.delete(
            res.subpath(format!("/segments/{}/groups/{}", segment.id, group.id)),
        )?;
    }
    println!("Deleted group [{}].", label);

    // Refresh segment.
    let updated = refresh_segment(session)?;
    session.context.write().unwrap().segment = Some(updated);
    Ok(())
}
