You are inside a **feature context** — these are also available:

- `FEATURE rename [name]` - stage a feature name change
- `FEATURE describe [desc]` - stage a feature description
- `FEATURE status on|off|archived` - stage a feature status
- `FEATURE server-side on|off` - stage server-side-only state
- `FEATURE tag tag1[, tag2, ...]` - stage adding tags (prefix a tag with `-` to remove it instead, e.g. `FEATURE tag -tag1`)
- `FEATURE progressive rules <w1>:<dur1> ... <100>` - stage a progressive rollout schedule for the feature's single alternative variant 
- `FEATURE progressive sample <n>` - stage the minimum number of distributed identities required before the schedule starts advancing
- `FEATURE progressive delete` - stage removing the progressive rollout entirely (clears the schedule and every environment's progression, not just this one)
- `FEATURE progressive status` - show the live progression status (applied immediately, not staged)

*Note: leaving off the trailing argument (e.g. `name` in `FEATURE rename`) opens your `$EDITOR` so you can edit the value interactively.*
