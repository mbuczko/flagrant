-- Adds progressive-rollout support: a single alternative variant's weight ramping up
-- over a defined schedule of steps. Rule definitions live as JSON directly on the
-- feature (shared across environments, and immediately tells you whether a feature is
-- "progressive"), matching how `feature_snapshots.state` is already kept untyped for
-- the same reason. Live progression (current step, when it last changed) is tracked
-- separately per (feature_id, environment_id), since the weight it drives is already
-- strictly per-environment in this system.
ALTER TABLE features ADD COLUMN rollout TEXT;

CREATE TABLE IF NOT EXISTS feature_rollout_state (
  feature_id     INTEGER NOT NULL REFERENCES features ON DELETE CASCADE,
  environment_id INTEGER NOT NULL REFERENCES environments ON DELETE CASCADE,
  current_step   INTEGER NOT NULL DEFAULT 0,
  last_change_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  created_at     DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,

  PRIMARY KEY (feature_id, environment_id)
);
