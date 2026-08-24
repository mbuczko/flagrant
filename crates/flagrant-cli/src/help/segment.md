A **segment** is a rule-based group of identities, useful for rolling a feature out to "beta testers", "premium plan users", a given environment, and so on, without touching individual identities one by one. A segment is made of one or more rule **groups** combined with AND / AND-NOT, and each group is itself a set of OR-ed **rules** matching on identity value, environment name, or a trait (equals, contains, greater/lower-than, in/not-in, and more, see `RULE` help for the full list).

Once you enter a feature context alongside a segment context, that segment can carry its own variant weight override for the feature (see `OVERRIDE add`/`OVERRIDE delete`), independent of the feature's general population, so a matched cohort can see a different split than everyone else.

- `SEGMENT add <name> [description]` - create a new segment and enter its context
- `SEGMENT list [pattern]` - list all segments in the current project
- `SEGMENT show [segment]` - show segment details
- `SEGMENT delete <segment>` - delete a segment
- `USE +<segment>` - switch into a segment context
