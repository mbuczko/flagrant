CREATE TABLE IF NOT EXISTS projects (
  project_id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL,
  created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS environments (
  environment_id INTEGER PRIMARY KEY AUTOINCREMENT,
  project_id INTEGER NOT NULL,
  name TEXT NOT NULL,
  description TEXT,
  created_at DATETIME DEFAULT CURRENT_TIMESTAMP,

  FOREIGN KEY (project_id) REFERENCES projects(project_id)
);

CREATE TABLE IF NOT EXISTS features (
  feature_id INTEGER PRIMARY KEY AUTOINCREMENT,
  project_id INTEGER NOT NULL,
  name TEXT NOT NULL,
  description TEXT,
  is_enabled BOOLEAN,
  version INTEGER NOT NULL DEFAULT 0,

  FOREIGN KEY (project_id) REFERENCES projects(project_id)
);

CREATE TABLE IF NOT EXISTS features_values(
  feature_id INTEGER NOT NULL,
  environment_id INTEGER NOT NULL,
  value TEXT,
  value_type TEXT NOT NULL DEFAULT 'text',

  PRIMARY KEY (feature_id, environment_id),
  FOREIGN KEY (feature_id) REFERENCES features(feature_id),
  FOREIGN KEY (environment_id) REFERENCES environments(environment_id)
);

CREATE TABLE IF NOT EXISTS variants (
  variant_id INTEGER PRIMARY KEY AUTOINCREMENT,
  feature_id TEXT NOT NULL,
  version INTEGER NOT NULL DEFAULT 0,

  FOREIGN KEY (feature_id) REFERENCES features(feature_id)
);

CREATE TABLE IF NOT EXISTS variants_weights (
  variant_id INTEGER NOT NULL,
  environment_id INTEGER NOT NULL,
  weight INTEGER NOT NULL,
  acc INTEGER NOT NULL DEFAULT 100,

  PRIMARY KEY (variant_id, environment_id),
  FOREIGN KEY (variant_id) REFERENCES variants(variant_id),
  FOREIGN KEY (environment_id) REFERENCES environments(environment_id)
);

CREATE TABLE IF NOT EXISTS variants_idents (
  identity TEXT NOT NULL PRIMARY KEY,
  variant_id TEXT NOT NULL,

  FOREIGN KEY (variant_id) REFERENCES variants(variant_id)
);
