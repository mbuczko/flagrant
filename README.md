# Flagrant - CLI-driven feature flagging system

> ⚠️ **Heavy development**: APIs, CLI commands and the on-disk schema are all still shifting release to release. Expect breaking changes without notice until this note goes away.

The feature-flagging space is already well served by excellent solutions like [Unleash](https://www.getunleash.io/) or [Flagsmith](https://www.flagsmith.com/), so why yet another one? Flagrant has an ambition to become the Redis of feature flagging - small, reliable, and completely CLI driven, providing everything needed to keep features under control without dragging in a dashboard-first, heavyweight platform.

Under the hood it's a Rust/Axum HTTP API backed by SQLite, driven day-to-day through a REPL-style CLI rather than a web UI - staged changes, tab completion and all.

Flagrant also tries its best to be a real-world showcase for a few other libraries of mine: [hugsqlx](https://github.com/mbuczko/hugsqlx) (compile-time-checked, macro-driven SQL queries) powers the entire persistence layer, [fancy-table](https://github.com/mbuczko/fancy-table) renders every table the CLI prints, and the CLI's readline stack is built on [my fork of rustyline](https://github.com/mbuczko/rustyline) (`feat/prompt-overlays` branch) adding dynamic prompt overlays for an inline help and an internal REPL tester.

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

## Concepts

Flagrant models four core entities - **features**, **variants**, **identities**, and **segments** - plus **overrides** that carve out exceptions to normal distribution. Everything is managed through the CLI's context-based `USE` commands: enter a context, stage changes, then apply them all at once with `COMMIT` (or throw them away with `DISCARD`).

### Context composition

Contexts compose: a **feature** context can be combined with either an **identity** or a **segment** context - but not both at once, since identity and segment are mutually exclusive with each other (entering one clears the other). The prompt reflects whatever's active:

``` sh
myproject/prod → ui_theme @ alice › ...
```

or

``` sh
myproject/prod → ui_theme + beta_testers › ...
```

Combine them in two steps:

```
FEATURE use <feature>
IDENTITY use <identity>   # or: SEGMENT use <name>
```

or in one, via a shortcut on `FEATURE use`:

```
FEATURE use <feature>@<identity>
FEATURE use <feature>+<segment>
```

A feature context alone lets you edit the feature itself (status, variants, tags, description, ...). Once an identity or segment context is also active, extra commands become available that only make sense across that combination - namely `OVERRIDE add [...]` / `OVERRIDE delete` (see Overrides below), which override that specific identity's or segment's variant assignment for the feature in context.

### Features & variants

A **feature** is a named flag scoped to a project + environment (e.g. `prod`/`staging`). Every feature has at least one **variant** - the *control* variant, always present, holding the feature's default value - plus any number of additional variants, each with its own value and a weight (0-100%). Weights across a feature's non-control variants describe how identities should be split between them; the control variant absorbs whatever's left. Distribution is handled by a self-balancing accumulator rather than a random number generator, so a given traffic split stays stable even as variants are added or weights change.

Enter a feature's context with:

```
FEATURE use <feature>
```

The prompt then shows the active feature, and these become available:

- `FEATURE status on|off|archived` to switch feature status to active (ON), inactive (OFF) or archived
- `FEATURE describe [description]` to add informative feature description
- `FEATURE server-side on|off` to change server-side only property of the feature 
- `VARIANT add <weight> <value>` to stage a new variant
- `VARIANT value <index> <value>` to modify value that variant conveys
- `VARIANT weight <index> [+/-]weight` to modify variant's weight - either explicitly or relatively to current value
- `VARIANT delete <index>` to stage variant for removal

None of this reaches the API until you run `COMMIT` (or `DISCARD` to drop it). Once commited, the change gets applied server-side in a single transaction.

### Identities & traits

An **identity** is a caller recognized across requests, identified by an arbitrary string value (a user id, session id, anything) sent via the `X-Flagrant-Identity` header. Identities can carry arbitrary typed **traits** (string/int/float/bool), used by segment rules to decide which cohort an identity belongs to. Once distributed to a variant for a feature, an identity keeps seeing that same variant on subsequent requests, unless something explicitly changes it - a weight change migrates a portion of identities, an override pins/unpins one, or its distribution is cleared outright.

Enter an identity's context with:

```
IDENTITY use <identity>
```

`IDENTITY add <identity> [trait:value ...]` creates one and switches into it in the same step. Inside the context:

- `IDENTITY trait <name:value|-name ...>` to stage trait changes/removals, e.g. `IDENTITY trait country:pl -org`
- `OVERRIDE add [value]` / `OVERRIDE delete` see Overrides below

### Segments

A **segment** is a project-scoped, rule-based group of identities - useful for rolling a feature out to "beta testers", "premium plan users", a given environment, etc, without touching individual identities one by one. A segment is made of one or more rule **groups** combined with AND / AND-NOT; each group is itself a set of OR-ed **rules** matching on identity value, environment name, or a trait (equals, contains, greater/lower-than, in/not-in, ...).

Enter a segment's context with:

```
SEGMENT use <name>
```

(mutually exclusive with an identity context - entering one clears the other). Inside the context:

- `GROUP add [--and|--and-not] [description]` to add a rule group
- `RULE add <group-label> <identity|trait|environment> <comparator> <value>` to add a condition to a group
- `GROUP delete <label>` / `RULE delete <group-label> <rule-index>` to remove them

### Overrides

Overrides bypass a feature's normal weighted distribution for a specific identity or a whole segment. Both require a feature + identity/segment context (see [Context composition](#context-composition)):

- **Identity override**: `OVERRIDE add [value]` pins that one identity to a specific variant of the feature, regardless of its weight-based assignment. Omit the value to open an editor listing every variant (marking the identity's current one), and pick from there. `OVERRIDE delete` releases the pin, freeing the identity to be redistributed on its next request.
- **Segment override**: `OVERRIDE add [variant-index weight]` overrides the feature's variant weights specifically for identities matching the segment, with its own independently-balanced control variant - so segment traffic can be split differently than the general population. Omit the arguments to open an editor for setting weights across all variants at once. `OVERRIDE delete` removes it, falling back to the feature's normal weights for that segment's identities.
- **Bulk clearing** (feature context only, no identity/segment context needed): `UNSET distribution <pattern>` clears the variant assignment for every identity whose value matches `pattern` (`*` as a wildcard), without deleting the identities or their traits - handy for forcing a whole cohort to be redistributed in case of emergency.

All staged changes across every active context - feature edits, identity/segment overrides, trait changes - are applied together with `COMMIT`, or dropped together with `DISCARD`.

### Snapshots

Every `COMMIT` that changes a feature - directly, or indirectly through a segment/identity override that touches it - automatically records a numbered **snapshot** of that feature's full state: its variants, any segment overrides (including the overriding segment's own rules, so it can be recreated if that segment is later deleted), and any pinned identity overrides. There's nothing to stage - it's just a side effect of committing, one snapshot per affected feature per commit, versions never reused even across restores.

Snapshots require a feature context (`FEATURE use <feature>`):

- `SNAPSHOT list` to see every version recorded for the feature, most recent first
- `SNAPSHOT show <version>` to inspect exactly what a version captured
- `SNAPSHOT describe <version> [comment]` to change a version's comment after the fact (omit the comment to edit it in an editor)
- `SNAPSHOT restore <version> [comment]` to bring the feature back to how it looked at that version

`COMMIT` itself takes an optional trailing comment (`COMMIT [comment]`), recorded on whichever snapshot(s) that commit produces.

Restoring is itself a commit, not a rewrite of history - it produces a brand-new snapshot matching the target version's state, so version numbers only ever go up. It reproduces variants (recreating one under a new id if it was deleted since), segment overrides (recreating the segment from its stored definition if it was deleted - though a still-existing segment's *rules* are left untouched, since rewriting them would silently change behaviour for every other feature that segment also overrides), and pinned identity overrides. Anything not part of the target version - like an override added after that point - is cleared rather than left behind. Organic (non-pinned) identity assignments are always cleared and left to redistribute on the next request, never restored.

## What's next

- [x] **Backend only flags** - allow to reach for certain flags only from the backend
- [x] **Snapshots** - capture and restore the full state of a feature definition and its overrides at a point in time
- [ ] **Scheduled feature-flags** - turn features on/off (or shift variant weights) on a schedule, not just on/off by hand
- [ ] **Progressive rollouts** - to automatically increase the amount of traffic to a specific flag variation over time 

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
