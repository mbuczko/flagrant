-- Records a full materialized snapshot of a feature's state (variants, segment
-- overrides, pinned identity overrides) right after each COMMIT that affects it.
-- Scoped per (feature_id, environment_id) since weights, control-variant value
-- selection, and segment override weights are all already per-environment.
CREATE TABLE IF NOT EXISTS feature_snapshots (
  snapshot_id    INTEGER PRIMARY KEY AUTOINCREMENT,
  feature_id     INTEGER NOT NULL REFERENCES features ON DELETE CASCADE,
  environment_id INTEGER NOT NULL REFERENCES environments ON DELETE CASCADE,
  version        INTEGER NOT NULL,
  comment        TEXT,
  state          TEXT NOT NULL,
  created_at     DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,

  UNIQUE(feature_id, environment_id, version)
);
