`USE` is the single command for switching context - which kind depends on the target's leading character: a bare name is a **feature**, `@name` an **identity**, `+name` a **segment**, `/name` an **environment**. A feature target can also carry an identity or segment shortcut in the same token; an environment switch stands alone.

- `USE <feature>` - switch into a feature context
- `USE @<identity>` - switch into an identity context
- `USE +<segment>` - switch into a segment context
- `USE /<environment>` - switch into a different environment
- `USE <feature>@<identity>` - switch into both at once
- `USE <feature>+<segment>` - switch into both at once

Identity and segment context are mutually exclusive - switching into one clears the other. Fails if there are uncommitted staged changes in whatever context is being left. Switching environment also clears identity context, and re-enters the previously active feature (if any) in the new environment. 

### Examples

- `USE ui_theme` - enter the `ui_theme` feature context
- `USE @alice` - enter identity `alice`'s context
- `USE +beta_testers` - enter the `beta_testers` segment context
- `USE ui_theme@alice` - enter `ui_theme`'s feature context together with `alice`'s identity one
- `USE /staging` - switch into the `staging` environment (same as `/staging`)
