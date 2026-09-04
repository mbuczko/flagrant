A **snapshot** is a numbered, point-in-time capture of a feature's full state - its variants, any segment overrides, and pinned identity overrides - recorded automatically every time a `COMMIT` changes that state. A structural segment change (a rename, or a group/rule edit) can cascade a fresh snapshot to every feature that segment overrides, even ones not in the current context, since each snapshot embeds the segment's definition as it was at that moment rather than referencing it live.

- `SNAPSHOT list` - list every snapshot recorded for the current feature, most recent first
- `SNAPSHOT show <version>` - print the full state captured by a snapshot version
- `SNAPSHOT describe <version> [comment]` - change a snapshot's comment
- `SNAPSHOT diff <version>` - show what differs between the feature's current state and a snapshot version
- `SNAPSHOT restore <version> [comment]` - restore the current feature to an earlier snapshot version

Restoring is itself a commit: it produces a brand-new snapshot matching the target version's state, rather than rewriting history in place - so the version you restored *from* is never lost, and a restore can itself be undone by restoring again.

Anywhere a `<version>` is expected, you can use `~N` (git `HEAD~N`-style) instead of an absolute number. The current state always matches the most recent snapshot (every commit that changes state immediately captures a new one), so `~0` is the latest snapshot itself, `~1` is one snapshot before it, `~2` two before, and so on.

### Examples

- `SNAPSHOT list` - see every recorded version for the feature in context
- `SNAPSHOT show 3` - inspect exactly what version 3 looked like
- `SNAPSHOT describe 3 "pre-launch baseline"` - label version 3 with a comment
- `SNAPSHOT diff 3` - preview what restoring to version 3 would change
- `SNAPSHOT restore 3` - roll the feature back to version 3, recorded as a new snapshot
- `SNAPSHOT describe ~0 "pre-launch baseline"` - label the snapshot a commit just produced, without looking up its version
- `SNAPSHOT restore ~1` - undo the last commit, rolling back to the snapshot before it
- `SNAPSHOT diff ~2` - compare the current state against the snapshot from two commits ago
