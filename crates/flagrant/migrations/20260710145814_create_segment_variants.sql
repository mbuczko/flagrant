CREATE TABLE IF NOT EXISTS segment_variants (
  segment_id     INTEGER NOT NULL REFERENCES segments ON DELETE CASCADE,
  feature_id     INTEGER NOT NULL REFERENCES features,
  variant_id     INTEGER NOT NULL REFERENCES variants,
  environment_id INTEGER NOT NULL REFERENCES environments,
  weight         INTEGER NOT NULL CHECK (weight >= 0 AND weight <= 100),

  UNIQUE(segment_id, feature_id, environment_id, variant_id),
  FOREIGN KEY (feature_id, variant_id) REFERENCES variants(feature_id, variant_id)
);
