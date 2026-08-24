A **feature** is a named flag scoped to a project + environment (e.g. `prod`, `staging`). It always has at least one variant: the *control* variant, holding its default value, present even before you add anything else. Add more variants and give each a weight (0 to 100), and traffic splits across them accordingly; the control variant quietly absorbs whatever weight is left over.

**Identities** (callers recognized across requests) get distributed across a feature's variants according to those weights, and keep seeing the same variant on later requests unless something changes it: editing a weight (which may trigger migration of a portion of identities from one variant to the other) or identity-override which pins identity explicitly to given variant.

- `FEATURE add <feature> <value>` - create a new feature with a default (control) value
- `FEATURE delete <feature>` - delete a feature
- `FEATURE show <feature>` - show feature details
- `FEATURE list [status|tag|pattern]` - list features
- `USE <feature>` - switch into a feature context

### Examples

- `FEATURE add theme dark` - create a `theme` feature with default value `dark`
- `FEATURE progressive rules 10:1m 30:2d 100` - turn progressive rollout: 10% for 1 minute, 30% for 2 days and then 100% 
- `FEATURE list status:on tag:beta` - filters combine, narrowing to enabled features tagged `beta`
