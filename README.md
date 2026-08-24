# Flagrant - CLI-driven feature flagging system

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

A single `USE` command handles all of it - what it switches to depends on the target's leading character: a bare name is a feature, `@name` an identity, `+name` a segment:

```
USE feature
USE @identity
USE +segment
```

Combine a feature switch with an identity/segment switch in one step by putting both in the same target:

```
USE feature@identity
USE feature+segment
```

A feature context alone lets you edit the feature itself (status, variants, tags, description, ...). Once an identity or segment context is also active, extra commands become available that only make sense across that combination - namely `OVERRIDE add [...]` / `OVERRIDE delete` (see Overrides below), which override that specific identity's or segment's variant assignment for the feature in context.

### Features & variants

A **feature** is a named flag scoped to a project, and automatically exists in every environment of that project (e.g. `prod`/`staging`) - there's no separate "create in staging, then create in prod" step. Every feature has at least one **variant** - the *control* variant, always present, holding the feature's default value - plus any number of additional variants, each with its own value and a weight (0-100%). Weights across a feature's non-control variants describe how identities should be split between them; the control variant absorbs whatever's left. Distribution is handled by a self-balancing accumulator rather than a random number generator, so a given traffic split stays stable even as variants are added or weights change.

Values and weights are shared across environments differently depending on which kind of variant they belong to:

- **Non-control variant value** is shared across every environment of the project - there's only one row for it, so changing a variant's value (`VARIANT value <index> <value>`) changes it everywhere at once.
- **Control variant value**, on the other hand, is independent per environment - each environment owns its own row, seeded from the feature's default value at creation time, so running `VARIANT value <index> <value>` against the control variant in one environment leaves every other environment's control value untouched.
- **Weight**, for both control and non-control variants, is always scoped per environment - so the very same variant (and, for non-control variants, the very same value) can be weighted differently in `prod` than in `staging`, letting you roll a feature out gradually per environment without duplicating variants.

Enter a feature's context with:

```
USE <feature>
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

### Server-side-only flags

A feature can be marked **server-side-only** with `FEATURE server-side on|off`. Such a feature is left out of the public feature-resolution endpoint (`GET /projects/{project}/envs/{environment}/features`) by default - useful for flags that should only ever be read by your own backend (internal rollout switches, backend-to-backend behaviour, etc.), never exposed to a browser/mobile client that only identifies itself via `X-Flagrant-Identity`.

To actually read srv-only features, a caller additionally sends an `Authorization: Bearer <token>` header, matching a per-project+environment `srv-token` configured server-side in `flagrant-api`'s TOML config file (`flagrant.toml` by default, or whatever path `FLAGRANT_CONFIG` points to):

```toml
[projects.my_project.envs.production]
srv-token = "prod-secret-token"
```

A valid token only ever *adds* srv-only features to the response on top of the normal ones - it never narrows it down to just those. No config entry (or an environment/project not listed at all) simply means no token unlocks srv-only features there, and the endpoint behaves as if the header was never sent - no error either way. Config is read once at startup; run `RELOAD` from the CLI (hits `POST /admin/reload`) to have a running server pick up changes to `flagrant.toml` - e.g. a rotated `srv-token` - without restarting it.

The same endpoint is also reachable over gRPC, as an alternative to HTTP - useful for backend-to-backend callers that prefer gRPC's binary framing, or that want to talk over a local Unix domain socket instead of a TCP port. It's opt-in: absent a `[grpc]` section in the TOML config, no gRPC listener is started at all. When enabled, it serves the exact same `FeatureResolver/GetFeatures` RPC as the HTTP route - `x-flagrant-identity` gRPC metadata takes the place of the `X-Flagrant-Identity` header, and a standard `authorization: Bearer <token>` metadata entry takes the place of the `Authorization` header for unlocking srv-only features - so behaviour (including caching and srv-token gating) never diverges between the two transports.

```toml
[grpc]
listen = "127.0.0.1:50051"
# or, for local IPC over a Unix domain socket instead of TCP:
# listen = "unix:/tmp/flagrant/grpc.sock"
```

Unlike `srv-token`, the gRPC listener address is read once at startup only - `RELOAD` picks up srv-token/Redis changes on a running server, but changing `[grpc].listen` requires a restart, since a bound listener can't be rebound onto a different address/socket path in place.

Both the Redis cache and the gRPC listener are also opt-in at *build* time, via the `redis` and `grpc` Cargo features on `flagrant-api` (both enabled by default) - independently of whether `[redis]`/`[grpc]` are actually present in `flagrant.toml`. Building with `cargo build -p flagrant-api --no-default-features` (optionally re-enabling just one, e.g. `--features redis`) drops the unused dependency (the `redis` client, or `tonic`/`prost` and the protobuf codegen build step) from the binary entirely - handy if you only ever run with one of them, or neither.

The always-on HTTP server's own listen address is configurable the same way, via an optional `[http]` section - absent (or with `[http]` omitted entirely), it defaults to `127.0.0.1:3030`:

```toml
[http]
listen = "0.0.0.0:3030"
```

Same restart caveat as `[grpc].listen`: read once at startup, not affected by `RELOAD`.

### Identities & traits

An **identity** is a caller recognized across requests, identified by an arbitrary string value (a user id, session id, anything) sent via the `X-Flagrant-Identity` header. Identities can carry arbitrary typed **traits** (string/int/float/bool), used by segment rules to decide which cohort an identity belongs to. Once distributed to a variant for a feature, an identity keeps seeing that same variant on subsequent requests, unless something explicitly changes it - a weight change migrates a portion of identities, an override pins/unpins one, or its distribution is cleared outright.

Enter an identity's context with:

```
USE @<identity>
```

`IDENTITY add <identity> [trait:value ...]` creates one and switches into it in the same step. Inside the context:

- `IDENTITY trait <name=value|-name ...>` to stage trait changes/removals, e.g. `IDENTITY trait country=pl -org`
- `OVERRIDE add [value]` / `OVERRIDE delete` see Overrides below

### Segments

A **segment** is a project-scoped, rule-based group of identities - useful for rolling a feature out to "beta testers", "premium plan users", a given environment, etc, without touching individual identities one by one. A segment is made of one or more rule **groups** combined with AND / AND-NOT; each group is itself a set of OR-ed **rules** matching on identity value, environment name, or a trait (equals, contains, greater/lower-than, in/not-in, ...).

Enter a segment's context with:

```
USE +<segment>
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

Snapshots require a feature context (`USE <feature>`):

- `SNAPSHOT list` to see every version recorded for the feature, most recent first
- `SNAPSHOT show <version>` to inspect exactly what a version captured
- `SNAPSHOT describe <version> [comment]` to change a version's comment after the fact (omit the comment to edit it in an editor)
- `SNAPSHOT restore <version> [comment]` to bring the feature back to how it looked at that version

`COMMIT` itself takes an optional trailing comment (`COMMIT [comment]`), recorded on whichever snapshot(s) that commit produces.

Restoring is itself a commit, not a rewrite of history - it produces a brand-new snapshot matching the target version's state, so version numbers only ever go up. It reproduces variants (recreating one under a new id if it was deleted since), segment overrides (recreating the segment from its stored definition if it was deleted - though a still-existing segment's *rules* are left untouched, since rewriting them would silently change behaviour for every other feature that segment also overrides), and pinned identity overrides. Anything not part of the target version - like an override added after that point - is cleared rather than left behind. Organic (non-pinned) identity assignments are always cleared and left to redistribute on the next request, never restored.

### Querying resolved values

`GET` and `GETALL` hit the same identity-facing evaluation endpoint SDKs use - read-only, nothing to stage or commit. Either takes its feature/identity from the current context if omitted, or explicitly overrides it:

- `GET [feature][@identity]` - resolve one feature's value for an identity.
- `GETALL [@identity]` - resolve every feature's value for an identity.

## What's next

- [x] **Backend only flags** - allow to reach for certain flags only within backend-to-backend communication
- [x] **Snapshots** - capture and restore the full state of a feature definition and its overrides at a point in time
- [ ] **Scheduled feature-flags** - turn features on/off (or shift variant weights) on a schedule, not just on/off by hand
- [x] **Progressive rollouts** - to automatically increase the amount of traffic to a specific flag variation over time 
- [x] **Caching layer (redis)** - to keep flags cached for given TTL and offload the hot-paths
- [x] **gRPC** - for backend-to-backend connection
- [x] **Docker image** 
- [ ] **Prometheus metrics**

Further out: analytics on flag exposure/conversion, and client SDKs beyond Rust (JVM, JS, Python).

# Architecture

To keep things simple yet still allow for extensibility, code is structured into the following crates:

- `flagrant` - core logic: entity models, SQL queries (via [hugsqlx](https://github.com/mbuczko/hugsqlx)), the weighted variant distributor, and the segment rule evaluator
- `flagrant-types` - core types shared across all other crates (`Feature`, `Variant`, `Identity`, `Segment`, request/patch payloads, ...)
- `flagrant-api` - the Axum HTTP server exposing both the client-facing feature-resolution endpoint (optionally also over gRPC, TCP or Unix socket - see [Server-side-only flags](#server-side-only-flags)) and the management API, with OpenAPI docs served via [Scalar](https://scalar.com/)
- `flagrant-cli` - the command-line REPL used to manage projects, environments, features, identities and segments, with all table output rendered via [fancy-table](https://github.com/mbuczko/fancy-table)
- `flagrant-client` - the HTTP client library used by `flagrant-cli` (and embeddable in other Rust apps) to talk to `flagrant-api`, with staging/caching baked in
- `flagrant-repl` - a small, reusable REPL framework (readline, tab completion, hinting, command parsing) that `flagrant-cli` is built on
- `flagrant-bombardier` - a load-testing tool that hammers a running `flagrant-api` with many concurrent identities to exercise/benchmark variant distribution
