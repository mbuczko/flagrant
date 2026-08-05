
## GROUP

Manage rule groups within the current segment (requires an active segment context).

- `GROUP add [--and|--and-not] [description]` - stage a new group on the current segment
- `GROUP show <label>` - show details of a group with its rules
- `GROUP describe <label> [desc]` - stage a group description change
- `GROUP delete <label>` - stage a group deletion by label

### Examples

- `GROUP add` - stage the segment's first group (no connector needed yet)
- `GROUP add --and "power users"` - stage another group that must *also* match, alongside the previous one(s)
- `GROUP add --and-not "internal testers"` - stage a group that must *not* match, excluding these identities
- `GROUP describe group-1 "power users"` - stage a description for `group-1`
- `GROUP delete group-2` - stage removal of `group-2`
