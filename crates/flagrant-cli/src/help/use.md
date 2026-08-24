`USE` is the single command for switching context - which kind depends on the target's leading character: a bare name is a **feature**, `@name` an **identity**, `+name` a **segment**. A feature target can also carry an identity or segment shortcut in the same token.

- `USE <feature>` - switch into a feature context
- `USE @<identity>` - switch into an identity context
- `USE +<segment>` - switch into a segment context
- `USE <feature>@<identity>` - switch into both at once
- `USE <feature>+<segment>` - switch into both at once

Identity and segment context are mutually exclusive - switching into one clears the other. Fails if there are uncommitted staged changes in whatever context is being left.

### Examples

- `USE ui_theme` - enter the `ui_theme` feature context
- `USE @alice` - enter identity `alice`'s context
- `USE +beta_testers` - enter the `beta_testers` segment context
- `USE ui_theme@alice` - enter `ui_theme`'s feature context together with `alice`'s identity one
