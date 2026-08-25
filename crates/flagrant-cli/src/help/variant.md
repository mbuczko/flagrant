A **variant** is one of the possible values a feature can resolve to. Every feature always has a *control* variant, added automatically and holding the feature's default value, and you can add any number of variants alongside it, each with its own value and a weight from 0 to 100%. Weights describe how identities should be split across the non-control variants; the control variant quietly absorbs whatever weight is left over.

Once an identity lands on a variant it keeps seeing that same one on later requests: given split stays stable even as variants are added or weights change later.

- `VARIANT add <weight> <value>` — stage a new variant addition
- `VARIANT show <index>` — print a variant's weight, full value, control status, and any identities explicitly pinned to it
- `VARIANT value <index> <value>` — stage a value change for an existing variant
- `VARIANT weight <index> [+/-]weight` — stage a weight change for an existing variant
- `VARIANT delete <index>` — stage a variant deletion

### Examples

- `VARIANT add 50 dark` - add a variant weighted 50%, holding the plain text value `dark`
- `VARIANT add 20` - add a variant weighted 20%, opening `$EDITOR` to type its value interactively
- `VARIANT add 30 json::{"color": "blue", "size": 12}` - add a variant with an explicit JSON value

*Note: values are text-typed by default, so use the `type::value` prefix to be explicit, e.g. `json::`, `toml::`, or `text::`.*
