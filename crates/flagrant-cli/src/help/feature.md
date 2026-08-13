
## FEATURE

A **feature** is a named flag scoped to a project + environment (e.g. `prod`, `staging`). It always has at least one **variant**: the *control* variant, holding its default value, present even before you add anything else. Add more variants and give each a weight (0 to 100), and traffic splits across them accordingly; the control variant quietly absorbs whatever weight is left over.

**Identities** (callers recognized across requests) get distributed across a feature's variants according to those weights, and keep seeing the same variant on later requests unless something changes it: editing a weight (which may trigger migration of a portion of identities from one variant to the other) or identity-override which pins identity explicitly to given variant.

- `FEATURE add <name> <value>` - create a new feature with a default (control) value
- `FEATURE list [status|tag|pattern]` - list features
- `FEATURE show <name>` - show feature details
- `FEATURE use <name>` - switch into a feature context
- `FEATURE delete <name>` - delete a feature

*Note: none of this reaches the API server right away. Staged changes to a feature (and its variants) are applied together, in a single transaction, when you run `COMMIT`, or dropped together with `DISCARD`.*

### Examples

- `FEATURE list` - list every feature
- `FEATURE list status:on` - only enabled features (`on`, `off`, or `archived`)
- `FEATURE list status:on tag:beta` - filters combine, narrowing to enabled features tagged `beta`

### Use-cases

- **Kill switch**: a feature with only the control variant. `FEATURE status off` disables it for everyone instantly, no variants needed.
- **A/B test**: two variants, `dark` and `light`, each weighted 50. Roughly half of identities land on one, half on the other, and each identity keeps seeing the same one.
- **Gradual rollout**: the control variant holds most of the weight, and a new variant starts small, say `weight: 5`. Raise it over time with `VARIANT weight <index> +10` until it reaches 100 and the rollout is complete.
- **Beta cohort**: normal weights favor the control variant for everyone, but a `beta_testers` segment gets its own override (`SEGMENT use beta_testers`, then `OVERRIDE add`) so testers see the new variant while everyone else keeps the default.

