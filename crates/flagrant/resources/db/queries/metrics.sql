-- :name fetch_identity_totals :<> :*
-- :doc Returns the number of identities in every environment of every project (zero for
-- environments with none)
SELECT p.name AS project, e.name AS environment, COUNT(i.identity_id) AS identities
FROM environments e
JOIN projects p ON p.project_id = e.project_id
LEFT JOIN identities i ON i.environment_id = e.environment_id
GROUP BY e.environment_id
ORDER BY p.name, e.name

-- :name fetch_variant_identity_counts :<> :*
-- :doc Returns the number of identities assigned to every variant of every feature, per
-- environment (zero for variants nobody is assigned to). The effective variant of an
-- assignment is migrated_id when set (a pending weight-migration), variant_id otherwise.
-- Non-control variants are shared across environments while a control variant belongs to
-- exactly one, hence the COALESCE on the variants join.
WITH assigned AS (
  SELECT environment_id, feature_id, COALESCE(migrated_id, variant_id) AS variant_id, COUNT(*) AS identities
  FROM identity_variants
  GROUP BY environment_id, feature_id, COALESCE(migrated_id, variant_id)
)
SELECT p.name AS project, e.name AS environment, f.name AS feature,
       v.variant_id, v.value AS variant, COALESCE(a.identities, 0) AS identities
FROM environments e
JOIN projects p ON p.project_id = e.project_id
JOIN features f ON f.project_id = p.project_id
JOIN variants v ON v.feature_id = f.feature_id
               AND COALESCE(v.environment_id, e.environment_id) = e.environment_id
LEFT JOIN assigned a ON a.environment_id = e.environment_id
                    AND a.feature_id = f.feature_id
                    AND a.variant_id = v.variant_id
ORDER BY p.name, e.name, f.name, v.variant_id

-- :name fetch_trait_identity_counts :<> :*
-- :doc Returns the number of identities carrying every trait, per environment of the trait's
-- project (zero for traits nobody carries), regardless of the trait's value
WITH carried AS (
  SELECT i.environment_id, it.trait_id, COUNT(*) AS identities
  FROM identity_traits it
  JOIN identities i ON i.identity_id = it.identity_id
  GROUP BY i.environment_id, it.trait_id
)
SELECT p.name AS project, e.name AS environment, t.name AS trait_name, COALESCE(c.identities, 0) AS identities
FROM environments e
JOIN projects p ON p.project_id = e.project_id
JOIN traits t ON t.project_id = p.project_id
LEFT JOIN carried c ON c.environment_id = e.environment_id AND c.trait_id = t.trait_id
ORDER BY p.name, e.name, t.name

-- :name fetch_segment_variant_identity_counts :<> :*
-- :doc Returns the number of identities attributed to a segment (identity_variants.segment_id)
-- for every variant of every feature the segment is involved with, per environment. A
-- segment is involved with a feature in an environment when it overrides its weights there
-- (so a fresh override nobody has hit yet is reported as zeros) or when identities are still
-- attributed to it. The effective variant follows migrated_id first, same as
-- fetch_variant_identity_counts.
WITH involved AS (
  SELECT vw.segment_id, v.feature_id, vw.environment_id
  FROM variant_weights vw
  JOIN variants v ON v.variant_id = vw.variant_id
  WHERE vw.segment_id IS NOT NULL
  UNION
  SELECT segment_id, feature_id, environment_id
  FROM identity_variants
  WHERE segment_id IS NOT NULL
),
assigned AS (
  SELECT environment_id, feature_id, segment_id, COALESCE(migrated_id, variant_id) AS variant_id, COUNT(*) AS identities
  FROM identity_variants
  WHERE segment_id IS NOT NULL
  GROUP BY environment_id, feature_id, segment_id, COALESCE(migrated_id, variant_id)
)
SELECT p.name AS project, e.name AS environment, f.name AS feature, s.name AS segment,
       v.variant_id, v.value AS variant, COALESCE(a.identities, 0) AS identities
FROM involved i
JOIN environments e ON e.environment_id = i.environment_id
JOIN projects p ON p.project_id = e.project_id
JOIN features f ON f.feature_id = i.feature_id
JOIN segments s ON s.segment_id = i.segment_id
JOIN variants v ON v.feature_id = f.feature_id
               AND COALESCE(v.environment_id, e.environment_id) = e.environment_id
LEFT JOIN assigned a ON a.environment_id = i.environment_id
                    AND a.feature_id = i.feature_id
                    AND a.segment_id = i.segment_id
                    AND a.variant_id = v.variant_id
ORDER BY p.name, e.name, f.name, s.name, v.variant_id

-- :name fetch_dirty_identity_counts :<> :*
-- :doc Returns, for every feature in every environment, the number of identity assignments
-- flagged segment_dirty - i.e. waiting for a segment change to be lazily re-evaluated the
-- next time the identity is read (zero when nothing is pending)
WITH dirty AS (
  SELECT environment_id, feature_id, COUNT(*) AS identities
  FROM identity_variants
  WHERE segment_dirty
  GROUP BY environment_id, feature_id
)
SELECT p.name AS project, e.name AS environment, f.name AS feature, COALESCE(d.identities, 0) AS identities
FROM environments e
JOIN projects p ON p.project_id = e.project_id
JOIN features f ON f.project_id = p.project_id
LEFT JOIN dirty d ON d.environment_id = e.environment_id AND d.feature_id = f.feature_id
ORDER BY p.name, e.name, f.name
