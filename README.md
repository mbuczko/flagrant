# Flagrant - CLI-driven feature flagging system

> ⚠️ **Heavy development**: APIs, CLI commands and the on-disk schema are all still shifting release to release. Expect breaking changes without notice until this note goes away.

The feature-flagging space is already well served by excellent solutions like [Unleash](https://www.getunleash.io/) or [Flagsmith](https://www.flagsmith.com/), so why yet another one? Flagrant has an ambition to become the Redis of feature flagging - small, reliable, and completely CLI driven, providing everything needed to keep features under control without dragging in a dashboard-first, heavyweight platform.

Under the hood it's a Rust/Axum HTTP API backed by SQLite, driven day-to-day through a REPL-style CLI rather than a web UI - staged changes, tab completion and all.

Flagrant also doubles as a real-world showcase for a few other libraries of mine: [hugsqlx](https://github.com/mbuczko/hugsqlx) (compile-time-checked, macro-driven SQL queries) powers the entire persistence layer, [fancy-table](https://github.com/mbuczko/fancy-table) renders every table the CLI prints, and the CLI's readline stack is built on [my fork of rustyline](https://github.com/mbuczko/rustyline) (`feat/prompt-overlays` branch) adding dynamic prompt overlays - wired in but not yet put to use, marked for an upcoming inline `HELP` and an internal REPL tester.

## What's there today

- **Multiple environments** per project (prod, dev, staging, ...), each with its own control values and weights
- **Multivariant features**, weighted and distributed to identities via a self-balancing accumulator (no external randomness/state needed)
- **Identities & traits** - callers are recognized across requests, with arbitrary typed traits (string/int/float/bool) attached to them
- **Identity overrides** - pin a specific identity to a specific variant, bypassing normal distribution
- **Segments** - project-scoped, rule-based groups of identities. A segment is made of one or more rule groups combined with AND/AND-NOT, each group itself a set of OR-ed rules matching on identity value, environment name, or an arbitrary trait (equals, contains, greater/lower-than, in/not-in, ...)
- **Segment overrides** - a segment can override a feature's variant weights for the identities that match it, with its own independently-balanced control variant
- A **rule evaluation engine** that resolves, for a given identity + environment + feature, which (if any) matching segment's weights should apply
- A CLI REPL (`flagrant-cli`) with staged/commit-style editing (`COMMIT`/`DISCARD`), tab completion, and rich table output for every entity above
- A fully **OpenAPI-documented HTTP API** - every endpoint is annotated via [`utoipa`](https://github.com/juhaku/utoipa) and served as an interactive, browsable reference through [Scalar](https://scalar.com/) at `/scalar` on a running `flagrant-api` instance

As it's written in Rust, Flagrant comes with low-level resource utilisation and "_blazingly fast_" mode switched on by default 😃

## What's next

- **Versioning** - track and roll back changes to features/segments over time (yes, just as git commits!)
- **Snapshots** - capture and restore the full state of a project/environment at a point in time
- **Scheduled feature-flags** - turn features on/off (or shift variant weights) on a schedule, not just on/off by hand
- **Socket-based communication protocol** - a lighter-weight, persistent alternative to HTTP for client libraries that need low-latency flag reads

Further out: analytics on flag exposure/conversion, and client libraries beyond Rust (JVM, JS, Python).

# Architecture

To keep things simple yet still allow for extensibility, code is structured into the following crates:

- `flagrant` - core logic: entity models, SQL queries (via [hugsqlx](https://github.com/mbuczko/hugsqlx)), the weighted variant distributor, and the segment rule evaluator
- `flagrant-types` - core types shared across all other crates (`Feature`, `Variant`, `Identity`, `Segment`, request/patch payloads, ...)
- `flagrant-api` - the Axum HTTP server exposing both the client-facing feature-resolution endpoint and the management API, with OpenAPI docs served via [Scalar](https://scalar.com/)
- `flagrant-cli` - the command-line REPL used to manage projects, environments, features, identities and segments, with all table output rendered via [fancy-table](https://github.com/mbuczko/fancy-table)
- `flagrant-client` - the HTTP client library used by `flagrant-cli` (and embeddable in other Rust apps) to talk to `flagrant-api`, with staging/caching baked in
- `flagrant-repl` - a small, reusable REPL framework (readline, tab completion, hinting, command parsing) that `flagrant-cli` is built on
- `flagrant-bombardier` - a load-testing tool that hammers a running `flagrant-api` with many concurrent identities to exercise/benchmark variant distribution
