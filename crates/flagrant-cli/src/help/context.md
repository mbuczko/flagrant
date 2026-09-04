Typing `/` switches the prompt to a `context>` overlay for changing which environment/feature/identity/segment is active, or clearing context entirely. Both the command keyword and its name argument tab-complete. A bare `/` with nothing typed after it lists the available commands.

- `/ENVIRONMENT <name>` - switch into a different environment
- `/FEATURE <name>` - switch into a feature context
- `/IDENTITY <name>` - switch into an identity context
- `/SEGMENT <name>` - switch into a segment context
- `/RESET` - clear feature, identity, and segment context

Identity and segment context are mutually exclusive - switching into one clears the other. Fails if there are uncommitted staged changes in whatever context is being left (or reset). Switching environment also clears identity context, and re-enters the previously active feature (if any) in the new environment.

`/ENVIRONMENT` with no name lists every environment in the project.

### Examples

- `/FEATURE ui_theme` - enter the `ui_theme` feature context
- `/IDENTITY alice` - enter identity `alice`'s context
- `/SEGMENT beta_testers` - enter the `beta_testers` segment context
- `/ENVIRONMENT staging` - switch into the `staging` environment
- `/ENVIRONMENT` - list every environment in the project
- `/RESET` - drop back to no feature/identity/segment context
