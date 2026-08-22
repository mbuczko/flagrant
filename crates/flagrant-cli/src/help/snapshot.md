A **snapshot** is a numbered, point-in-time capture of a feature's full state - its variants, any segment overrides, and pinned identity overrides - recorded automatically every time a `COMMIT` changes that state. A structural segment change (a rename, or a group/rule edit) can cascade a fresh snapshot to every feature that segment overrides, even ones not in the current context, since each snapshot embeds the segment's definition as it was at that moment rather than referencing it live.

- `SNAPSHOT list` - list every snapshot recorded for the current feature, most recent first
- `SNAPSHOT show <version>` - print the full state captured by a snapshot version
- `SNAPSHOT describe <version> [comment]` - change a snapshot's comment
- `SNAPSHOT restore <version> [comment]` - restore the current feature to an earlier snapshot version

Restoring is itself a commit: it produces a brand-new snapshot matching the target version's state, rather than rewriting history in place - so the version you restored *from* is never lost, and a restore can itself be undone by restoring again.

### Examples

- `SNAPSHOT list` - see every recorded version for the feature in context
- `SNAPSHOT show 3` - inspect exactly what version 3 looked like
- `SNAPSHOT describe 3 "pre-launch baseline"` - label version 3 with a comment
- `SNAPSHOT restore 3` - roll the feature back to version 3, recorded as a new snapshot
