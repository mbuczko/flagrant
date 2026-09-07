Typing an entity command with just a name (no other operation) switches into that entity's context - the same mechanic for all following commands, with tab-completion built-in:

- `ENVIRONMENT <name>` - switch into a different environment
- `FEATURE <name>` - switch into a feature context
- `IDENTITY <name>` - switch into an identity context
- `SEGMENT <name>` - switch into a segment context
- `SEGMENT use <name>` - same as above

If a name happens to match one of that command's other sub-commands (e.g. a feature literally named `list`), the bare form can't reach it. Use the explicit `use` form instead: `FEATURE use <name>`, `IDENTITY use <name>`, `SEGMENT use <name>`, `ENVIRONMENT use <name>` - unambiguous regardless of what `<name>` is.

Identity and segment context are mutually exclusive - switching into one clears the other. Fails if there are uncommitted staged changes in whatever context is being left (or reset). Switching environment also clears identity context, and re-enters the previously active feature (if any) in the new environment.

`RESET` clears feature, identity, and segment context.

### Examples

- `FEATURE ui_theme` - enter the `ui_theme` feature context
- `IDENTITY alice` - enter identity `alice`'s context
- `SEGMENT beta_testers` - enter the `beta_testers` segment context
- `ENVIRONMENT staging` - switch into the `staging` environment
- `ENVIRONMENT` - list every environment in the project
- `RESET` - drop back to no feature/identity/segment context
