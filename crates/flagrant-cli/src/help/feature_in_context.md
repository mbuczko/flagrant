You are inside a **feature context** — these are also available:

- `FEATURE rename [name]` - stage a feature name change
- `FEATURE describe [desc]` - stage a feature description
- `FEATURE status on|off|archived` - stage a feature status
- `FEATURE server-side on|off` - stage server-side-only state
- `FEATURE tag tag1[, tag2, ...]` - stage adding tags (prefix a tag with `-` to remove it instead, e.g. `FEATURE tag -tag1`)

*Note: leaving off the trailing argument (e.g. `name` in `FEATURE rename`) opens your `$EDITOR` so you can edit the value interactively.*
