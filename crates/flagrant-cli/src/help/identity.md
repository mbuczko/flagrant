An **identity** is a caller recognized across requests, identified by an arbitrary string value (a user id, session id, anything) sent via a header. Identities can carry arbitrary typed **traits** (string, int, float, or bool), attached with `IDENTITY trait`. Once an identity is distributed to a variant for a feature, it keeps seeing that same variant on later requests, unless something explicitly changes it: a weight change migrates a portion of identities, `OVERRIDE add`/`OVERRIDE delete` pins or unpins one directly, or `UNSET distribution <pattern>` (feature context only) clears a whole cohort so it gets redistributed.

**Segments** build on identities and their traits: a segment's rules can match on identity value, environment, or a trait, grouping many identities into a cohort without editing them one by one. Combine a feature context with an identity context (or a segment context instead, but not both at once) to unlock overrides scoped to that specific identity or cohort.

- `IDENTITY add <identity> [trait=value ...]` - create or upsert an identity
- `IDENTITY list [trait|pattern]` - list up to 10 identities, optionally filtered
- `IDENTITY show [identity]` - show an identity with its traits
- `IDENTITY delete <identity>` - delete identities matching a pattern (`*` wildcard)
