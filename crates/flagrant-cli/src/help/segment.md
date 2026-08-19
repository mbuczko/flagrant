
## SEGMENT

Manage project-scoped rule sets for identity grouping.

A **segment** is a rule-based group of identities, useful for rolling a feature out to "beta testers", "premium plan users", a given environment, and so on, without touching individual identities one by one. A segment is made of one or more rule **groups** combined with AND / AND-NOT, and each group is itself a set of OR-ed **rules** matching on identity value, environment name, or a trait (equals, contains, greater/lower-than, in/not-in, and more, see `RULE` help for the full list).

Once you enter a feature context alongside a segment context, that segment can carry its own variant weight override for the feature (see `OVERRIDE add`/`OVERRIDE delete`), independent of the feature's general population, so a matched cohort can see a different split than everyone else.

- `SEGMENT add <name> [description]` - create a new segment and enter its context
- `SEGMENT list [pattern]` - list all segments in the current project
- `SEGMENT show [name]` - show segment details
- `SEGMENT delete <name>` - delete a segment
- `SEGMENT use <name>` - switch into a segment context

### Use-cases

- **Beta program**: a segment with one group matching identities whose `beta` trait equals `true`. Give it its own variant override so only beta testers see the new variant, while everyone else keeps the default.
- **Staging behavior**: a segment matching the `staging` environment, used to force a different weight split there regardless of which identity is calling.
- **Paying customers**: a segment with a group matching trait `plan` `in` `["pro","enterprise"]`, so a feature can be rolled out to paying users only, ahead of the general population.
- **Regional exclusion**: two groups combined with AND-NOT, one matching a trait `country` `exactly_matches` `de`, another excluding a handful of internal test identities, so the segment covers "Germany, except our own test accounts".
