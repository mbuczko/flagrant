You are inside feature **{feature}** + segment **{segment}** - `OVERRIDE add`/`OVERRIDE delete` let you override this feature's variant weights for identities matching this segment.

### Examples

- `OVERRIDE add` - open an interactive menu to adjust variant weights for this feature+segment
- `OVERRIDE add 1 30` - set variant #1's weight to 30% for this segment
- `OVERRIDE delete` - remove all segment-scoped weight overrides for this feature
