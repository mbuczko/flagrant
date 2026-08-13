
## IDENTITY

Manage identities and their traits.

An **identity** is a caller recognized across requests, identified by an arbitrary string value (a user id, session id, anything) sent via a header. Identities can carry arbitrary typed **traits** (string, int, float, or bool), attached with `IDENTITY trait`. Once an identity is distributed to a variant for a feature, it keeps seeing that same variant on later requests, unless something explicitly changes it: a weight change migrates a portion of identities, `OVERRIDE add`/`OVERRIDE delete` pins or unpins one directly, or `UNSET distribution <pattern>` (feature context only) clears a whole cohort so it gets redistributed.

**Segments** build on identities and their traits: a segment's rules can match on identity value, environment, or a trait, grouping many identities into a cohort without editing them one by one. Combine a feature context with an identity context (or a segment context instead, but not both at once) to unlock overrides scoped to that specific identity or cohort.

- `IDENTITY add <identity> [trait=value ...]` - create or upsert an identity
- `IDENTITY list [trait|pattern]` - list up to 10 identities, optionally filtered
- `IDENTITY show [identity]` - show an identity with its traits
- `IDENTITY delete <identity>` - delete identities matching a pattern (`*` wildcard)
- `IDENTITY use <identity>` - switch into an identity context

### Use-cases

- **Sticky assignment**: no overrides at all, just weights. An identity lands on a variant the first time it's seen, and keeps seeing that same one on every later request.
- **VIP pinning**: `FEATURE use theme@alice` then `OVERRIDE add dark` forces `alice` onto the `dark` variant regardless of the feature's normal weights.
- **Trait-based cohort**: tag identities with `IDENTITY trait plan=enterprise`, then build a `SEGMENT` whose rule matches `trait:plan exactly_matches enterprise`, so a feature can be rolled out to that cohort ahead of everyone else.
- **Forced redistribution**: after a feature's variants change significantly, `UNSET distribution <pattern>` clears a whole cohort's assignments so they get redistributed under the new weights on their next request.
