
## GROUP

Manage rule groups within the current segment (requires an active segment context).

A **group** is a set of OR-ed rules: it matches if *any* of its rules matches. A segment is one or more groups combined with AND / AND-NOT, so groups are the layer between individual rules and the segment as a whole, letting you build boolean logic that a single flat rule list couldn't express: "match any of these, but only if that other condition also holds, and never if this third one does". The first group added is the segment's head and needs no connector; every group after it must say how it relates to what came before, via `--and` or `--and-not` (the former is assumed by default).

- `GROUP add [--and|--and-not] [description]` - stage a new group on the current segment
- `GROUP show <label>` - show details of a group with its rules
- `GROUP describe <label> [description]` - stage a group description change
- `GROUP delete <label>` - stage a group deletion by label

### Examples

- `GROUP add` - stage the segment's first group (no connector needed yet)
- `GROUP add --and "power users"` - stage another group that must *also* match, alongside the previous one(s)
- `GROUP add --and-not "internal testers"` - stage a group that must *not* match, excluding these identities
- `GROUP describe group-1 "power users"` - stage a description for `group-1`
- `GROUP delete group-2` - stage removal of `group-2`
