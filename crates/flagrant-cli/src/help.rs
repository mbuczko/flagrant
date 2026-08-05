//! Content for the `?` help overlay - shows a command family summary, extended with
//! notes about commands unlocked by the current session context. Content lives in the
//! `.md` files under `help/` and is rendered with basic Markdown formatting (bold,
//! italic, inline code, lists) via `termimad`.

use flagrant_client::connection::Connection;
use flagrant_repl::session::Session;
use termimad::MadSkin;

pub const TOPICS: &[&str] = &["FEATURE", "SEGMENT", "IDENTITY", "VARIANT", "GROUP", "RULE"];

const FEATURE: &str = include_str!("help/feature.md");
const FEATURE_IN_CONTEXT: &str = include_str!("help/feature_in_context.md");
const FEATURE_NO_CONTEXT: &str = include_str!("help/feature_no_context.md");
const FEATURE_SEGMENT_NOTE: &str = include_str!("help/feature_segment_note.md");

const SEGMENT: &str = include_str!("help/segment.md");
const SEGMENT_IN_CONTEXT: &str = include_str!("help/segment_in_context.md");
const SEGMENT_NO_CONTEXT: &str = include_str!("help/segment_no_context.md");

const IDENTITY: &str = include_str!("help/identity.md");
const IDENTITY_IN_CONTEXT: &str = include_str!("help/identity_in_context.md");
const IDENTITY_NO_CONTEXT: &str = include_str!("help/identity_no_context.md");

const VARIANT: &str = include_str!("help/variant.md");
const VARIANT_NO_CONTEXT: &str = include_str!("help/variant_no_context.md");

const GROUP: &str = include_str!("help/group.md");
const GROUP_NO_CONTEXT: &str = include_str!("help/group_no_context.md");

const RULE: &str = include_str!("help/rule.md");
const RULE_NO_CONTEXT: &str = include_str!("help/rule_no_context.md");

pub fn show(topic: &str, session: &Session<Connection>) -> anyhow::Result<()> {
    let topic = topic.trim().to_uppercase();
    let ctx = session.context.read().unwrap();

    let text = match topic.as_str() {
        "" => format!("Available help topics: {}", topic_list()),
        "FEATURE" => feature_help(&ctx),
        "SEGMENT" => segment_help(&ctx),
        "IDENTITY" => identity_help(&ctx),
        "VARIANT" => variant_help(&ctx),
        "GROUP" => group_help(&ctx),
        "RULE" => rule_help(&ctx),
        _ => format!("No help for `{topic}`. Available topics: {}", topic_list()),
    };

    render(&text);
    Ok(())
}

fn topic_list() -> String {
    TOPICS
        .iter()
        .map(|t| format!("`{t}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn render(markdown: &str) {
    MadSkin::default().print_text(markdown);
}

/// Appended to every topic's help when both a feature and a segment context are
/// active, since that combination unlocks feature+segment-scoped overrides.
fn feature_segment_note(ctx: &Connection) -> Option<String> {
    match (&ctx.feature, &ctx.segment) {
        (Some(feature), Some(segment)) => Some(
            FEATURE_SEGMENT_NOTE
                .replace("{feature}", &feature.name)
                .replace("{segment}", &segment.name),
        ),
        _ => None,
    }
}

fn feature_help(ctx: &Connection) -> String {
    let mut text = format!(
        "{FEATURE}\n{}",
        if ctx.feature.is_some() {
            FEATURE_IN_CONTEXT
        } else {
            FEATURE_NO_CONTEXT
        }
    );
    if let Some(note) = feature_segment_note(ctx) {
        text.push('\n');
        text.push_str(&note);
    }
    text
}

fn segment_help(ctx: &Connection) -> String {
    let mut text = format!(
        "{SEGMENT}\n{}",
        if ctx.segment.is_some() {
            SEGMENT_IN_CONTEXT
        } else {
            SEGMENT_NO_CONTEXT
        }
    );
    if let Some(note) = feature_segment_note(ctx) {
        text.push('\n');
        text.push_str(&note);
    }
    text
}

fn identity_help(ctx: &Connection) -> String {
    format!(
        "{IDENTITY}\n{}",
        if ctx.identity.is_some() {
            IDENTITY_IN_CONTEXT
        } else {
            IDENTITY_NO_CONTEXT
        }
    )
}

fn variant_help(ctx: &Connection) -> String {
    let mut text = String::from(VARIANT);
    if ctx.feature.is_none() {
        text.push('\n');
        text.push_str(VARIANT_NO_CONTEXT);
    }
    if let Some(note) = feature_segment_note(ctx) {
        text.push('\n');
        text.push_str(&note);
    }
    text
}

fn group_help(ctx: &Connection) -> String {
    let mut text = String::from(GROUP);
    if ctx.segment.is_none() {
        text.push('\n');
        text.push_str(GROUP_NO_CONTEXT);
    }
    text
}

fn rule_help(ctx: &Connection) -> String {
    let mut text = String::from(RULE);
    if ctx.segment.is_none() {
        text.push('\n');
        text.push_str(RULE_NO_CONTEXT);
    }
    text
}
