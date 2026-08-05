You are inside feature **{feature}** + segment **{segment}** — `SET`/`UNSET override` let you override this feature's variant weights for identities matching this segment.

### Examples

- `SET override` - open `$EDITOR` to edit all variant weights for this feature+segment
- `SET override 1 30` - set variant #1's weight to 30% for this segment
- `UNSET override` - remove all segment-scoped weight overrides for this feature
