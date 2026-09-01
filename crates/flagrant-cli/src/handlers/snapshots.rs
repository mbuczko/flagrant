use anyhow::bail;
use fancy_table::{Align, FancyTable, FancyTableOpts, Layout, Width};
use flagrant_client::connection::Connection;
use flagrant_repl::{command::Arg, session::Session};
use flagrant_types::{
    Snapshot,
    payload::{RestoreRequest, UpdateSnapshotCommentPayload},
};

use crate::{
    handlers::{
        features::fetch_feature,
        internal::{index, stage},
        prompt_line,
    },
    printer::{menu, tabular::Tabular},
};

fn current_feature_id(session: &Session<Connection>) -> anyhow::Result<i32> {
    session
        .context
        .read()
        .unwrap()
        .feature
        .as_ref()
        .map(|f| f.id)
        .ok_or_else(|| {
            anyhow::anyhow!("Not in a feature context. Use \"USE <feature>\" to set a context.")
        })
}

fn parse_version(args: &[Arg], usage: &str) -> anyhow::Result<i32> {
    args.get(1)
        .ok_or_else(|| anyhow::anyhow!("Usage: {usage}"))?
        .to_string()
        .parse()
        .map_err(|_| anyhow::anyhow!("Version must be a number."))
}

/// Lists every snapshot recorded for the current feature, most recent first.
pub fn list(_args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let feature_id = current_feature_id(session)?;
    let ctx = session.context.read().unwrap();
    let path = ctx
        .env_resource()
        .subpath(format!("/features/{feature_id}/snapshots"));
    let snapshots: Vec<Snapshot> = ctx.client.get(path)?;
    drop(ctx);

    if snapshots.is_empty() {
        println!("No snapshots recorded yet for this feature.");
        return Ok(());
    }

    let rows: Vec<_> = snapshots
        .iter()
        .map(|s| {
            [
                format!("v{}", s.version),
                s.comment.clone().unwrap_or_default(),
                s.created_at.to_string(),
            ]
        })
        .collect();

    FancyTable::create(FancyTableOpts::default())
        .add_column_named_with_align("VERSION".into(), Layout::Fixed(10), Align::Left)
        .add_column_named_with_align("COMMENT".into(), Layout::Expandable(50), Align::Left)
        .add_column_named_with_align("CREATED AT".into(), Layout::Fixed(22), Align::Left)
        .width(Width::Percentage(100))
        .build()
        .render(rows);

    Ok(())
}

/// Prints the full state captured by a specific snapshot version - variants, segment
/// overrides (with their full definition as captured, not the segment's current live
/// state), and pinned identity overrides.
pub fn show(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let feature_id = current_feature_id(session)?;
    let version = parse_version(args, "SNAPSHOT show <version>")?;

    let ctx = session.context.read().unwrap();
    let path = ctx
        .env_resource()
        .subpath(format!("/features/{feature_id}/snapshots/{version}"));
    let snapshot: Snapshot = ctx.client.get(path)?;
    drop(ctx);

    let state = snapshot.parsed_state()?;
    snapshot.display(None, &state);
    Ok(())
}

/// Changes a snapshot's comment - the only field of a recorded snapshot that's ever
/// mutated after the fact; state and version are fixed at capture time. Applied
/// immediately (not staged) - like `IDENTITY delete`/`SEGMENT delete`, a past snapshot
/// isn't "the current entity in context" to stage a patch against and COMMIT later.
///
/// Expected args: `[version] [comment]`
///
/// When the version is omitted, opens an interactive menu listing every snapshot
/// recorded for the current feature to choose from instead. When the comment is
/// omitted, prompts for it inline, pre-filled with the current comment.
pub fn describe(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    let feature_id = current_feature_id(session)?;
    let ctx = session.context.read().unwrap();

    let version: i32 = match args.get(1) {
        Some(v) => v
            .to_string()
            .parse()
            .map_err(|_| anyhow::anyhow!("Version must be a number."))?,
        None => {
            let path = ctx
                .env_resource()
                .subpath(format!("/features/{feature_id}/snapshots"));
            let snapshots: Vec<Snapshot> = ctx.client.get(path)?;

            if snapshots.is_empty() {
                bail!("No snapshots recorded yet for this feature.");
            }

            let rows: Vec<(String, String)> = snapshots
                .iter()
                .map(|s| {
                    (
                        format!("v{} ({})", s.version, s.created_at),
                        s.comment.clone().unwrap_or_default(),
                    )
                })
                .collect();
            let options: Vec<(String, i32)> = menu::align_rows(&rows)
                .into_iter()
                .zip(snapshots.iter().map(|s| s.version))
                .collect();

            menu::select("Describe which snapshot", &options, None)?
                .ok_or_else(|| anyhow::anyhow!("No snapshot selected."))?
        }
    };

    let path = ctx
        .env_resource()
        .subpath(format!("/features/{feature_id}/snapshots/{version}"));

    let comment = match args.get(2) {
        Some(c) => c.to_string(),
        None => {
            let current: Snapshot = ctx.client.get(path.clone())?;
            let Some(edited) =
                prompt_line("New comment", current.comment.as_deref().unwrap_or(""))?
            else {
                println!("Cancelled.");
                return Ok(());
            };
            edited
        }
    };

    let updated: Snapshot = ctx.client.patch(
        path,
        UpdateSnapshotCommentPayload {
            comment: (!comment.is_empty()).then_some(comment),
        },
    )?;
    drop(ctx);

    println!("Snapshot v{} comment updated.", updated.version);
    Ok(())
}

/// Restores the current feature to the state captured by a given snapshot version.
///
/// Restoring is itself a commit: it produces a brand-new snapshot whose state matches the
/// target version, rather than rewriting history in place. Refuses to run while any
/// context has uncommitted staged changes, mirroring `COMMIT`/`DISCARD`'s own guard.
pub fn restore(args: &[Arg], session: &Session<Connection>) -> anyhow::Result<()> {
    stage::ensure_no_pending(session)?;

    let feature_id = current_feature_id(session)?;
    let version = parse_version(args, "SNAPSHOT restore <version> [comment]")?;
    let comment = args.get(2).map(|a| a.to_string());

    let ctx = session.context.read().unwrap();
    let path = ctx.env_resource().subpath(format!(
        "/features/{feature_id}/snapshots/{version}/restore"
    ));
    let snapshot: Snapshot = ctx.client.post(path, RestoreRequest { comment })?;
    let feature_name = ctx.feature.as_ref().unwrap().name.clone();

    drop(ctx);

    // Restoring changes the feature server-side (tags, variants, version, ...) - refetch
    // it so the session's cached context doesn't keep showing pre-restore state (which
    // would also send a now-stale `version` on the next COMMIT, wrongly rejected as a
    // conflict).
    let feature = fetch_feature(&feature_name, session)?;
    let mut ctx = session.context.write().unwrap();

    ctx.feature = Some(feature);
    index::rebuild(&mut ctx);

    drop(ctx);

    println!(
        "Restored to v{version} - recorded as new snapshot v{}.",
        snapshot.version
    );
    Ok(())
}
