# Flagrant - CLI-driven feature flagging system

## Table of contents

- [What's there today](#whats-there-today)
- [Running with Docker](#running-with-docker)
- [Concepts](#concepts)
  - [Context composition](#context-composition)
  - [Interactive prompts](#interactive-prompts)
  - [Features & variants](#features--variants)
  - [Server-side-only flags](#server-side-only-flags)
  - [Metrics](#metrics)
  - [Identities & traits](#identities--traits)
  - [Segments](#segments)
  - [Overrides](#overrides)
  - [Snapshots](#snapshots)
  - [Querying resolved values](#querying-resolved-values)
- [Using the Rust SDK (`flagrant-sdk`)](#using-the-rust-sdk-flagrant-sdk)
  - [Caching](#caching)
- [What's next](#whats-next)
- [Architecture](#architecture)

The feature-flagging space is already well served by excellent solutions like [Unleash](https://www.getunleash.io/) or [Flagsmith](https://www.flagsmith.com/), so why yet another one? Flagrant has an ambition to become the Redis of feature flagging - small, reliable, and completely CLI driven, providing everything needed to keep features under control without dragging in a dashboard-first, heavyweight platform.

Under the hood it's a Rust/Axum HTTP API backed by SQLite, driven day-to-day through a REPL-style CLI rather than a web UI - staged changes, tab completion and all.

Flagrant also tries its best to be a real-world showcase for a few other libraries of mine: [hugsqlx](https://github.com/mbuczko/hugsqlx) (compile-time-checked, macro-driven SQL queries) powers the entire persistence layer, [fancy-table](https://github.com/mbuczko/fancy-table) renders every table the CLI prints, and the CLI's readline stack is built on [my fork of rustyline](https://github.com/mbuczko/rustyline) (`feat/prompt-overlays` branch) adding dynamic prompt overlays (eg. for inline help).

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

https://github.com/user-attachments/assets/6e26ae6a-4964-4428-8da0-8c9e9fa2f703

## Running with Docker

The published image bundles both `flagrant-api` (the long-running server, and the container's default entrypoint) and `flagrant-cli`, so a single container is enough to try Flagrant end-to-end - no local Rust toolchain or separate build step needed:

```sh
docker pull mbuczko/flagrant-api:latest
docker run -d --name flagrant-api -p 3030:3030 mbuczko/flagrant-api:latest
docker exec -it flagrant-api flagrant-cli -p demo
```

The first two commands pull and start the API server. The third opens a `flagrant-cli` REPL inside that same running container - `-p demo` opens project `demo`, creating it (and its first environment) if it doesn't already exist. No `-h` flag is needed since `flagrant-cli`'s default host, `http://localhost:3030`, already points at the API server sharing that container.

By default the image stores its SQLite database at `/data/flagrant.db` (declared as a `VOLUME`, so it survives container restarts) and reads its server config from `/etc/flagrant/flagrant.toml` (a minimal default baked in - see [Server-side-only flags](#server-side-only-flags) for what can go in there). Both are overridable via environment variables:

- `FLAGRANT_DB` - path to the SQLite database file
- `FLAGRANT_CONFIG` - path to the TOML config file

When running `flagrant-api` outside Docker, both variables can also be placed in a `.env` file in the working directory (or a parent of it) instead of being exported in the shell. Variables already set in the environment take precedence over the `.env` file.

```sh
docker run -d --name flagrant-api -p 3030:3030 \
  -v $(pwd)/data:/data \
  -v $(pwd)/my-flagrant.toml:/etc/flagrant/my-flagrant.toml:ro \
  -e FLAGRANT_DB=/data/my-flagrant.db \
  -e FLAGRANT_CONFIG=/etc/flagrant/my-flagrant.toml \
  mbuczko/flagrant-api:latest
```

The base image is distroless (no shell, no `mkdir`), so whatever path either variable points at needs to already exist inside the container - bind-mounting it in, as above, is the simplest way, since Docker creates the host-side path for you before the container starts.

## Concepts

Flagrant models four core entities - **features**, **variants**, **identities**, and **segments** - plus **overrides** that carve out exceptions to normal distribution. Everything is managed through the CLI's context-based commands: enter a context, stage changes, then apply them all at once with `COMMIT` (or throw them away with `DISCARD`).

### Context composition

Contexts compose: a **feature** context can be combined with either an **identity** or a **segment** context - but not both at once, since identity and segment are mutually exclusive with each other (entering one clears the other). The prompt reflects whatever's active:

``` sh
myproject/prod → ui_theme @ alice › ...
```

or

``` sh
myproject/prod → ui_theme + beta_testers › ...
```

Typing an entity's command with just a name switches into that entity's context - the same mechanic works for all following commands, with tab-completion built-in:

```
FEATURE feature
IDENTITY identity
SEGMENT segment
ENVIRONMENT environment
```

If a name happens to collide with one of that command's other sub-commands (e.g. a feature literally named `list`), the bare form can't reach it. Use the explicit `use` op instead, which is unambiguous regardless of what the name is: `FEATURE use list`, `IDENTITY use show`, `SEGMENT use add`, `ENVIRONMENT use dev`.

Identity and segment context are independent of feature context, so combining a feature switch with an identity/segment switch takes two commands, e.g. `FEATURE feature` then `IDENTITY identity`. An environment switch re-enters the previously active feature in the new environment. `ENVIRONMENT` with no name lists every environment in the project. `RESET` drops feature, identity, and segment context all at once.

A feature context alone lets you edit the feature itself (status, variants, tags, description, ...). Once an identity or segment context is also active, extra commands become available that only make sense across that combination - namely `OVERRIDE set [...]` / `OVERRIDE delete` (see Overrides below), which override that specific identity's or segment's variant assignment for the feature in context.

### Interactive prompts

Most commands that take an identifying argument (an index, a label, a name) or a piece of free text (a value, a name, a description, a comment) make that trailing argument optional. Leave it out and the CLI fills the gap interactively instead of erroring out:

- An identifying argument - when omitted, opens an arrow-key menu listing the matching entities to choose from - e.g. `VARIANT delete`, `GROUP delete`, `RULE delete` (group and rule index are each independently optional there). The number/label you'd have picked by hand and the one a menu selection resolves to are always interchangeable - selecting from the menu is just a shortcut for typing the same argument.
- A free-text value - when omitted, drops into a single-line prompt pre-filled with the current value (or blank for a new one) so it can be edited in place - e.g. `FEATURE describe`, `VARIANT add`/`value`, `SNAPSHOT describe`.

Both can combine: a command missing everything (e.g. bare `VARIANT value`) shows the menu first, then prompts for the value once a variant's picked.

### Features & variants

A **feature** is a named flag scoped to a project, and automatically exists in every environment of that project (e.g. `prod`/`staging`) - there's no separate "create in staging, then create in prod" step. Every feature has at least one **variant** - the *control* variant, always present, holding the feature's default value - plus any number of additional variants, each with its own value and a weight (0-100%). Weights across a feature's non-control variants describe how identities should be split between them; the control variant absorbs whatever's left. Distribution is handled by a self-balancing accumulator rather than a random number generator, so a given traffic split stays stable even as variants are added or weights change.

Values and weights are shared across environments differently depending on which kind of variant they belong to:

- **Non-control variant value** is shared across every environment of the project - there's only one row for it, so changing a variant's value (`VARIANT value <index> <value>`) changes it everywhere at once.
- **Control variant value**, on the other hand, is independent per environment - each environment owns its own row, seeded from the feature's default value at creation time, so running `VARIANT value <index> <value>` against the control variant in one environment leaves every other environment's control value untouched.
- **Weight**, for both control and non-control variants, is always scoped per environment - so the very same variant (and, for non-control variants, the very same value) can be weighted differently in `prod` than in `staging`, letting you roll a feature out gradually per environment without duplicating variants.

Enter a feature's context with:

```
FEATURE <feature>
```

The prompt then shows the active feature, and these become available:

- `FEATURE status on|off|archived` to switch feature status to active (ON), inactive (OFF) or archived
- `FEATURE describe [description]` to add informative feature description
- `FEATURE server-side on|off` to change server-side only property of the feature 
- `VARIANT add <weight> [value]` to stage a new variant
- `VARIANT value [index] [value]` to modify value that variant conveys
- `VARIANT weight [index [+/-]weight]` to modify variant's weight - either explicitly or relatively to current value
- `VARIANT delete [index]` to stage variant for removal (the control variant is never offered - it's managed automatically)

None of this reaches the API until you run `COMMIT` (or `DISCARD` to drop it). Once commited, the change gets applied server-side in a single transaction.

### Server-side-only flags

A feature can be marked **server-side-only** with `FEATURE server-side on|off`. Such a feature is left out of the public feature-resolution endpoint (`GET /projects/{project}/envs/{environment}/features`) by default - useful for flags that should only ever be read by your own backend (internal rollout switches, backend-to-backend behaviour, etc.), never exposed to a browser/mobile client that only identifies itself via `X-Flagrant-Identity`.

To actually read srv-only features, a caller additionally sends an `Authorization: Bearer <token>` header, matching a per-project+environment `srv-token` configured server-side in `flagrant-api`'s TOML config file (`flagrant.toml` by default, or whatever path `FLAGRANT_CONFIG` points to):

```toml
[projects."demo/prod"]
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

### Metrics

`flagrant-api` can expose [Prometheus](https://prometheus.io/) metrics. It's opt-in: absent a `[metrics]` section in the TOML config, nothing is listened on and nothing is computed. Metrics are served from their own listener (rather than the main HTTP one) since they carry project, feature and trait names plus identity counts - bind it to an internal interface and point your scraper at `GET /metrics`:

```toml
[metrics]
listen = "127.0.0.1:9090"
# how often the gauges are recomputed from the database (default: 30)
refresh-seconds = 30
```

The gauges are recomputed by a background task every `refresh-seconds`, not on each scrape, so adding scrapers never adds database load - a scrape returns values at most one interval old. Same restart caveat as `[grpc].listen`: read once at startup, not affected by `RELOAD`.

| Metric | Labels | Meaning |
|---|---|---|
| `flagrant_identities_total` | `project`, `environment` | identities in the environment |
| `flagrant_variant_identities` | `project`, `environment`, `feature`, `variant_id`, `variant` | identities currently assigned to the variant |
| `flagrant_variant_identities_ratio` | same as above | share (0-1) of the feature's assigned identities that got the variant |
| `flagrant_trait_identities` | `project`, `environment`, `trait` | identities carrying the trait, whatever its value |
| `flagrant_segment_identities` | `project`, `environment`, `feature`, `segment` | identities currently attributed to the segment for the feature |
| `flagrant_segment_variant_identities` | `project`, `environment`, `feature`, `segment`, `variant_id`, `variant` | the same, split per variant - to verify a segment's weight override is actually honored |
| `flagrant_segment_dirty_identities` | `project`, `environment`, `feature` | identity assignments waiting to be re-evaluated after a segment change |

A few things worth knowing when reading them:

- Variant counts follow an identity's *effective* variant, so a pending weight-shift migration is already reflected before the identity is next read.
- Only identities already resolved for a feature are assigned to one of its variants. The `_ratio` is relative to that assigned population (a feature's ratios sum to 1, or are all 0 while nobody is assigned), not to `flagrant_identities_total`. Multiply by 100 in PromQL (or use Grafana's `percentunit`) for a percentage.
- The `variant` label is the variant's value, cut down to its first line and 64 characters; `variant_id` is the stable key if the value gets edited.
- Segment metrics are only reported for a segment and feature it overrides (or still holds identities of), and a fresh override nobody has hit yet shows up as zeros. Identities are attributed to a segment per feature, lazily, when they are read - there is no feature-independent "members of segment X" count.
- `flagrant_segment_dirty_identities` is the backlog of identity assignments flagged by a segment change, settled one by one as each identity is next read. It should drain to 0 as traffic arrives; a value that stays high means those identities aren't being read (or the segment keeps being edited). It is reported for every feature, zero when nothing is pending.
- Traits are counted per trait *name* only, never per value - values are free-form, and a label per value would make the number of series unbounded.

The endpoint is also opt-in at *build* time, via the `metrics` Cargo feature on `flagrant-api` (enabled by default), same as `redis` and `grpc` above.

### Identities & traits

An **identity** is a caller recognized across requests, identified by an arbitrary string value (a user id, session id, anything) sent via the `X-Flagrant-Identity` header. Identities can carry arbitrary typed **traits** (string/int/float/bool), used by segment rules to decide which cohort an identity belongs to. Once distributed to a variant for a feature, an identity keeps seeing that same variant on subsequent requests, unless something explicitly changes it - a weight change migrates a portion of identities, an override pins/unpins one, or its distribution is cleared outright.

Enter an identity's context with:

```
IDENTITY <identity>
```

`IDENTITY add <identity> [trait:value ...]` creates one and switches into it in the same step. Inside the context:

- `IDENTITY trait <name=value|-name ...>` to stage trait changes/removals, e.g. `IDENTITY trait country=pl -org`
- `OVERRIDE set [variant-index]` / `OVERRIDE delete` see Overrides below

### Segments

A **segment** is a project-scoped, rule-based group of identities - useful for rolling a feature out to "beta testers", "premium plan users", a given environment, etc, without touching individual identities one by one. A segment is made of one or more rule **groups** combined with AND / AND-NOT; each group is itself a set of OR-ed **rules** matching on identity value, environment name, or a trait (equals, contains, greater/lower-than, in/not-in, ...).

Enter a segment's context with:

```
SEGMENT <segment>
```

(mutually exclusive with an identity context - entering one clears the other). Inside the context:

- `GROUP add [--and|--and-not] [description]` to add a rule to the group
- `GROUP rejoin [label] [and|and-not]` to change a non-head group's connector (the first group has none)
- `GROUP delete [label]` to remove the group of given (autogenerated) label (eg. `group-1`)
- `RULE add <group-label> <identity|trait|environment> <comparator> <value>` to add a new rule to a group
- `RULE delete [group-label] [rule-index]` to remove a rule from given group

Available rule comparators:
- `exactly_matches` / `does_not_match` - value must (not) match the subject (eg. `environment exactly_matches prod`)
- `contains` / `does_not_contain` - value must (not) be a substring of the subject (eg. `identity contains test`)
- `greater_than`/ `greater_equal_than` - subject must be greater/greater-or-equal then numerical value 
- `lower_than`/ `lower_equal_than` - subject must be lower/lower-or-equal than numerical value
- `in`, `not_in` - subject must/must not be one of the elements of value - this requires `<value>` to be a JSON array, e.g. `["pro", "enterprise"]`

### Overrides

Overrides bypass a feature's normal weighted distribution for a specific identity or a whole segment. Both require a feature + identity/segment context (see [Context composition](#context-composition)):

- **Identity override**: `OVERRIDE set [variant-index]` pins that one identity to a specific variant of the feature (by its display index, same numbering as `FEATURE show`), regardless of its weight-based assignment. Omit the index to pick from an interactive menu instead, listing every variant with the identity's current one marked. `OVERRIDE delete` releases the pin, freeing the identity to be redistributed on its next request.
- **Segment override**: `OVERRIDE set [variant-index weight]` overrides the feature's variant weights specifically for identities matching the segment, with its own independently-balanced control variant - so segment traffic can be split differently than the general population. Omit both arguments to open an interactive menu instead: navigate variants with the arrow keys and adjust each one's weight up/down by 5% at a time, with the control variant's weight auto-balancing live as you go. `OVERRIDE delete` removes it, falling back to the feature's normal weights for that segment's identities.
- **Bulk clearing** (feature context only, no identity/segment context needed): `UNSET distribution <pattern>` clears the variant assignment for every identity whose value matches `pattern` (`*` as a wildcard), without deleting the identities or their traits - handy for forcing a whole cohort to be redistributed in case of emergency.

All staged changes across every active context - feature edits, identity/segment overrides, trait changes - are applied together with `COMMIT`, or dropped together with `DISCARD`.

### Snapshots

Every `COMMIT` that changes a feature - directly, or indirectly through a segment/identity override that touches it - automatically records a numbered **snapshot** of that feature's full state: its variants, any segment overrides (including the overriding segment's own rules, so it can be recreated if that segment is later deleted), and any pinned identity overrides. There's nothing to stage - it's just a side effect of committing, one snapshot per affected feature per commit, versions never reused even across restores.

Snapshots require a feature context (`FEATURE <feature>`):

- `SNAPSHOT list` to see every version recorded for the feature, most recent first
- `SNAPSHOT show <version>` to inspect exactly what a version captured
- `SNAPSHOT describe [version] [comment]` to change a version's comment after the fact
- `SNAPSHOT restore <version> [comment]` to bring the feature back to how it looked at that version

`COMMIT` itself takes an optional trailing comment (`COMMIT [comment]`), recorded on whichever snapshot(s) that commit produces.

Restoring is itself a commit, not a rewrite of history - it produces a brand-new snapshot matching the target version's state, so version numbers only ever go up. It reproduces variants (recreating one under a new id if it was deleted since), segment overrides (recreating the segment from its stored definition if it was deleted - though a still-existing segment's *rules* are left untouched, since rewriting them would silently change behaviour for every other feature that segment also overrides), and pinned identity overrides. Anything not part of the target version - like an override added after that point - is cleared rather than left behind. Organic (non-pinned) identity assignments are always cleared and left to redistribute on the next request, never restored.

### Querying resolved values

`GET` and `GETALL` hit the same identity-facing evaluation endpoint SDKs use - read-only, nothing to stage or commit. Either takes its feature/identity from the current context if omitted, or explicitly overrides it:

- `GET [feature][@identity]` - resolve one feature's value for an identity.
- `GETALL [@identity]` - resolve every feature's value for an identity.

## Using the Rust SDK (`flagrant-sdk`)

`flagrant-sdk` is the Rust client external apps embed to resolve features for an identity - the very same identity-facing endpoint `GET`/`GETALL` hit in the CLI (see [Querying resolved values](#querying-resolved-values)), and nothing else. It has no notion of projects/features/segments management at all - that's `flagrant-cli`'s job, talking to `flagrant-api` through the separate, admin-only `flagrant-client` crate (see [Architecture](#architecture)).

A `FlagrantClient` (or its async counterpart, `AsyncFlagrantClient`) pairs one of three interchangeable transports with a project name, and exposes exactly one method:

```rust
fn get_features(&self, environment: &str, identity: &str) -> Result<Features>
```

`Features` is an immutable snapshot of the resolved feature set - a cheaply-cloneable view that derefs to `[FeatureResponse]`, so it iterates and indexes like a plain slice. Cloning shares one backing allocation instead of copying every entry, which is what keeps cache hits (see [Caching](#caching)) down to a refcount bump.

Project is fixed for the client's lifetime - an app embedding it is typically wired to one project - while environment is passed per call, since the same client commonly serves more than one (`dev`, `prod`, ...).

Each transport sits behind its own Cargo feature, so a consumer only pulls in what it actually needs:

| Feature | Transport | Client type |
|---|---|---|
| `http-blocking` | `reqwest::blocking` | `FlagrantClient<HttpBlockingTransport>` |
| `http-async` | `reqwest` (async) | `AsyncFlagrantClient<HttpAsyncTransport>` |
| `grpc` | `tonic`, dialing the same `FeatureResolver/GetFeatures` RPC `flagrant-api` serves (see [Server-side-only flags](#server-side-only-flags)) | `AsyncFlagrantClient<GrpcTransport>` |

```toml
[dependencies]
flagrant-sdk = { version = "0.0.40", features = ["http-blocking"] }  # or "http-async", or "grpc"
```

**HTTP, blocking:**

```rust
use flagrant_sdk::{FlagrantClient, HttpBlockingTransport};

let transport = HttpBlockingTransport::new("http://localhost:3030".into(), None);
let client = FlagrantClient::new(transport, "my-project".into());

let features = client.get_features("prod", "alice")?;
```

**HTTP, async:**

```rust
use flagrant_sdk::{AsyncFlagrantClient, HttpAsyncTransport};

let transport = HttpAsyncTransport::new("http://localhost:3030".into(), None);
let client = AsyncFlagrantClient::new(transport, "my-project".into());

let features = client.get_features("prod", "alice").await?;
```

**gRPC:**

```rust
use flagrant_sdk::{AsyncFlagrantClient, GrpcTransport};

let transport = GrpcTransport::connect("http://localhost:50051", None).await?;
let client = AsyncFlagrantClient::new(transport, "my-project".into());

let features = client.get_features("prod", "alice").await?;
```

Every transport's second constructor argument is an optional bearer/srv-token, exactly like the `Authorization: Bearer <token>` header/`authorization` metadata described in [Server-side-only flags](#server-side-only-flags) - pass `Some("prod-secret-token".into())` to unlock server-side-only features on top of the normal response, or `None` for a regular client that only ever sees public features.

See `crates/flagrant-sdk/examples/grpc_get_features.rs` for a complete, runnable example.

### Caching

Both client types take an optional `with_cache(cache_size, cache_ttl)` builder call, caching resolved features per `(environment, identity)` pair with LRU eviction once `cache_size` is reached:

```rust
use std::num::NonZeroUsize;
use std::time::Duration;

use flagrant_sdk::{FlagrantClient, HttpBlockingTransport};

let transport = HttpBlockingTransport::new("http://localhost:3030".into(), None);
let client = FlagrantClient::new(transport, "my-project".into())
    .with_cache(NonZeroUsize::new(1_000).unwrap(), Some(Duration::from_secs(30)));

let features = client.get_features("prod", "alice")?;
```

A hit within `cache_ttl` only bumps recency, without a round-trip to the transport, and hands back a clone of the cached `Features` snapshot - a refcount bump, not a deep copy of every `FeatureResponse`. An entry past its TTL is reported as a miss and refetched, but isn't evicted - it's kept around as a fallback, so if that refetch then fails (e.g. the connection to the Flagrant API died), the stale value is served instead of an error. Pass `None` as `cache_ttl` for a cache that never expires entries on its own, only via LRU eviction once `cache_size` is reached.

Caching is entirely in-process and client-local - unrelated to the server-side `redis` caching layer mentioned in [What's next](#whats-next), which caches at `flagrant-api` instead.

## What's next

- [x] **Backend only flags** - allow to reach for certain flags only within backend-to-backend communication
- [x] **Snapshots** - capture and restore the full state of a feature definition and its overrides at a point in time
- [ ] **Scheduled feature-flags** - turn features on/off (or shift variant weights) on a schedule, not just on/off by hand
- [x] **Progressive rollouts** - to automatically increase the amount of traffic to a specific flag variation over time 
- [x] **Caching layer (redis)** - to keep flags cached for given TTL and offload the hot-paths
- [x] **gRPC** - for backend-to-backend connection
- [x] **Docker multi-arch (amd64/arm64) image**
- [x] **k8s helm chart**
- [x] **Prometheus metrics** - identities per variant, trait and segment; runtime (cache/HTTP) metrics to follow

Further out: analytics on flag exposure/conversion, and client SDKs for other languages (JVM, JS, [Python](https://github.com/mbuczko/flagrant-client-python)) - see [Using the Rust SDK](#using-the-rust-sdk-flagrant-sdk) for the one that exists today.

# Architecture

To keep things simple yet still allow for extensibility, code is structured into the following crates:

- `flagrant` - core logic: entity models, SQL queries (via [hugsqlx](https://github.com/mbuczko/hugsqlx)), the weighted variant distributor, and the segment rule evaluator
- `flagrant-types` - core types shared across all other crates (`Feature`, `Variant`, `Identity`, `Segment`, request/patch payloads, ...)
- `flagrant-proto` - the `.proto` contract for the public feature-resolution gRPC service, compiled once into both client and server stubs, shared by `flagrant-api` and `flagrant-sdk`
- `flagrant-api` - the Axum HTTP server exposing both the client-facing feature-resolution endpoint (optionally also over gRPC, TCP or Unix socket - see [Server-side-only flags](#server-side-only-flags)) and the management API, with OpenAPI docs served via [Scalar](https://scalar.com/)
- `flagrant-sdk` - the embeddable Rust SDK external apps use to resolve features for an identity only, over a choice of transports (HTTP blocking/async, gRPC) - see [Using the Rust SDK](#using-the-rust-sdk-flagrant-sdk)
- `flagrant-cli` - the command-line REPL used to manage projects, environments, features, identities and segments, with all table output rendered via [fancy-table](https://github.com/mbuczko/fancy-table)
- `flagrant-client` - the admin-only HTTP client library `flagrant-cli` is built on, for everything management-related (projects, features, segments, identities, snapshots, ...); not meant to be embedded in other apps - that's what `flagrant-sdk` is for
- `flagrant-repl` - a small, reusable REPL framework (readline, tab completion, hinting, command parsing) that `flagrant-cli` is built on
- `flagrant-bombardier` - a load-testing tool that hammers a running `flagrant-api` with many concurrent identities to exercise/benchmark variant distribution
