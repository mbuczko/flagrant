
## VARIANT

Manage a feature's variants (requires an active feature context).

A **variant** is one of the possible values a feature can resolve to. Every feature always has a *control* variant, added automatically and holding the feature's default value, and you can add any number of variants alongside it, each with its own value and a weight from 0 to 100. Weights describe how identities should be split across the non-control variants; the control variant quietly absorbs whatever weight is left over.

Once an identity lands on a variant it keeps seeing that same one on later requests: distribution uses a self-balancing accumulator rather than a fresh random roll each time, so a given split stays stable even as variants are added or weights change later.

- `VARIANT add <weight> <value>` — stage a new variant addition
- `VARIANT value <index> <value>` — stage a value change for an existing variant
- `VARIANT weight <index> [+/-]weight` — stage a weight change for an existing variant
- `VARIANT delete <index>` — stage a variant deletion

### Examples

- `VARIANT add 50 dark` - add a variant weighted 50, holding the plain text value `dark`
- `VARIANT add 20` - add a variant weighted 20, opening `$EDITOR` to type its value interactively
- `VARIANT add 30 json::{"color": "blue", "size": 12}` - add a variant with an explicit JSON value (values are treated as plain text by default, so use the `type::value` prefix to be explicit, e.g. `json::`, `toml::`, or `text::`)

### Example scenarios

- **Pricing experiment**: three variants holding different price points (`9.99`, `14.99`, `19.99`), weighted evenly, to see which converts best.
- **Copy testing**: two variants holding different banner text, weighted 50/50, so half of identities see one wording and half see the other.
- **Config rollout**: a variant holding a new JSON config blob, e.g. `json::{"retries": 5}`, started at a low weight and raised gradually with `VARIANT weight <index> +10` as confidence grows, while the control variant keeps serving the old config to everyone else.
