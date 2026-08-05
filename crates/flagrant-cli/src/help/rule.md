
## RULE

Manage rules within a segment's groups (requires an active segment context).

A **rule** is a single condition, matching on identity value, environment name, or a trait, using a comparator such as equals, contains, greater/lower-than, or in/not-in. Rules are what a group is built from: all rules within a group are OR-ed together, and groups themselves combine with AND / AND-NOT to build up the segment's overall matching logic (see `GROUP` help). A rule is the smallest building block, so getting these right is what makes a segment match the identities you actually intend.

- `RULE add <group-label> <identity|trait|environment> <comparator> <value>` - stage a new rule
- `RULE show <group-label> <index>` - show details of a single rule
- `RULE delete <group-label> <index>` - stage a rule deletion by 1-based index
- `RULE value <group-label> <index> [value]` - stage a value change for a rule
- `RULE comparator <group-label> <index> [comparator]` - stage a comparator change

### Examples

- `RULE add group-1 identity exactly_matches alice` - match the identity `alice` exactly
- `RULE add group-1 trait:plan exactly_matches pro` - match identities whose `plan` trait equals `pro`
- `RULE add group-1 environment exactly_matches prod` - match only within the `prod` environment
- `RULE add group-2 trait:age greater_than 18` - match identities whose `age` trait is greater than `18`
- `RULE value group-1 1 pro` - change rule 1 in `group-1` to match value `pro`
- `RULE comparator group-1 1 contains` - change rule 1 in `group-1` to use the `contains` comparator

Available comparators:
- `exactly_matches`
- `does_not_match`
- `contains` and `does_not_contain`
- `greater_than` and `greater_equal_than`
- `lower_than` and `lower_equal_than`
- `in` and `not_in` - expect a JSON array value, e.g. `["a","b"]`

### Example scenarios

- **Plan tiers in one go**: `RULE add group-1 trait:plan in ["pro","enterprise"]` matches either plan with a single rule, instead of one rule per tier OR-ed together.
- **Named exceptions**: several rules in the same group, e.g. `identity exactly_matches bob` and `identity exactly_matches carol`, OR-ed together to explicitly include a handful of test accounts.
- **Age range**: a single group can only OR its rules together, so a range needs two groups combined with AND instead, one with `trait:age greater_equal_than 18`, another with `trait:age lower_than 65`, so only identities in that band match both.
- **Environment-only targeting**: a lone rule, `environment exactly_matches staging`, in its own group, so the segment matches every identity, but only while calling from staging.
