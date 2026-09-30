use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::Duration,
};

use axum::{Router, extract::State, http::header, response::IntoResponse, routing::get};
use flagrant::models::metrics::{
    DirtyIdentityCount, IdentityTotal, SegmentVariantIdentityCount, TraitIdentityCount,
    VariantIdentityCount, dirty_identity_counts, identity_totals, segment_variant_identity_counts,
    trait_identity_counts, variant_identity_counts,
};
use flagrant_types::VariantValue;
use prometheus::{Encoder, GaugeVec, Opts, Registry, TextEncoder};
use sqlx::SqlitePool;

use crate::config::MetricsConfig;

/// Variant values can be up to 1024 chars, far too long for a label - `variant_id` is the
/// stable key, the (truncated) value is only there to make the series readable.
const VARIANT_LABEL_MAX_CHARS: usize = 64;

/// Point-in-time numbers read from the database, before they're turned into gauges.
#[derive(Debug, Default)]
struct IdentityCounts {
    totals: Vec<IdentityTotal>,
    variants: Vec<VariantIdentityCount>,
    traits: Vec<TraitIdentityCount>,
    segment_variants: Vec<SegmentVariantIdentityCount>,
    dirty: Vec<DirtyIdentityCount>,
}

impl IdentityCounts {
    async fn collect(pool: &SqlitePool) -> anyhow::Result<Self> {
        let mut conn = pool.acquire().await?;

        Ok(Self {
            totals: identity_totals(&mut conn).await?,
            variants: variant_identity_counts(&mut conn).await?,
            traits: trait_identity_counts(&mut conn).await?,
            segment_variants: segment_variant_identity_counts(&mut conn).await?,
            dirty: dirty_identity_counts(&mut conn).await?,
        })
    }

    /// Renders the counts in the Prometheus text exposition format.
    ///
    /// Gauges live in a registry built from scratch on every call rather than a long-lived
    /// one that is reset and refilled: a scrape can then never observe a half-populated
    /// registry, and features/variants/traits deleted since the previous refresh disappear
    /// instead of lingering at their last value.
    fn render(&self) -> anyhow::Result<String> {
        let registry = Registry::new();

        let identities_total = gauge_vec(
            &registry,
            "flagrant_identities_total",
            "Identities in the environment",
            &["project", "environment"],
        )?;
        let variant_identities = gauge_vec(
            &registry,
            "flagrant_variant_identities",
            "Identities currently assigned to the feature variant",
            VARIANT_LABELS,
        )?;
        let variant_ratio = gauge_vec(
            &registry,
            "flagrant_variant_identities_ratio",
            "Share (0-1) of the feature's assigned identities that got the variant",
            VARIANT_LABELS,
        )?;
        let trait_identities = gauge_vec(
            &registry,
            "flagrant_trait_identities",
            "Identities carrying the trait, whatever its value",
            &["project", "environment", "trait"],
        )?;
        let segment_identities = gauge_vec(
            &registry,
            "flagrant_segment_identities",
            "Identities currently attributed to the segment for the feature",
            &["project", "environment", "feature", "segment"],
        )?;
        let segment_variant_identities = gauge_vec(
            &registry,
            "flagrant_segment_variant_identities",
            "Identities attributed to the segment that are assigned to the feature variant",
            SEGMENT_VARIANT_LABELS,
        )?;
        let segment_dirty_identities = gauge_vec(
            &registry,
            "flagrant_segment_dirty_identities",
            "Identity assignments of the feature awaiting re-evaluation after a segment change",
            &["project", "environment", "feature"],
        )?;

        for row in &self.totals {
            identities_total
                .with_label_values(&[&row.project, &row.environment])
                .set(row.identities as f64);
        }

        // A feature's ratios are relative to the identities assigned for that feature (not
        // to every identity in the environment, most of which may never have been
        // evaluated against it), so they always sum to 1 - or are all 0 when nobody is
        // assigned yet.
        let mut assigned: HashMap<(&str, &str, &str), i64> = HashMap::new();
        for row in &self.variants {
            *assigned
                .entry((&row.project, &row.environment, &row.feature))
                .or_default() += row.identities;
        }
        for row in &self.variants {
            let variant_id = row.variant_id.to_string();
            let variant = variant_label(&row.variant);
            let labels = [
                row.project.as_str(),
                row.environment.as_str(),
                row.feature.as_str(),
                variant_id.as_str(),
                variant.as_str(),
            ];
            let feature_total = assigned[&(&*row.project, &*row.environment, &*row.feature)];
            let ratio = match feature_total {
                0 => 0.0,
                total => row.identities as f64 / total as f64,
            };

            variant_identities
                .with_label_values(&labels)
                .set(row.identities as f64);
            variant_ratio.with_label_values(&labels).set(ratio);
        }

        for row in &self.traits {
            trait_identities
                .with_label_values(&[&row.project, &row.environment, &row.trait_name])
                .set(row.identities as f64);
        }

        // A segment's total for a feature is just the sum of its variants' counts - every
        // identity attributed to the segment holds exactly one variant of that feature.
        let mut attributed: HashMap<(&str, &str, &str, &str), i64> = HashMap::new();
        for row in &self.segment_variants {
            let variant_id = row.variant_id.to_string();
            let variant = variant_label(&row.variant);

            segment_variant_identities
                .with_label_values(&[
                    &row.project,
                    &row.environment,
                    &row.feature,
                    &row.segment,
                    &variant_id,
                    &variant,
                ])
                .set(row.identities as f64);
            *attributed
                .entry((&row.project, &row.environment, &row.feature, &row.segment))
                .or_default() += row.identities;
        }
        for ((project, environment, feature, segment), identities) in attributed {
            segment_identities
                .with_label_values(&[project, environment, feature, segment])
                .set(identities as f64);
        }

        for row in &self.dirty {
            segment_dirty_identities
                .with_label_values(&[&row.project, &row.environment, &row.feature])
                .set(row.identities as f64);
        }

        Ok(TextEncoder::new().encode_to_string(&registry.gather())?)
    }
}

const VARIANT_LABELS: &[&str] = &["project", "environment", "feature", "variant_id", "variant"];

const SEGMENT_VARIANT_LABELS: &[&str] = &[
    "project",
    "environment",
    "feature",
    "segment",
    "variant_id",
    "variant",
];

/// A variant's value in its compact single-line form, cut down to label size.
fn variant_label(value: &VariantValue) -> String {
    truncate(value.bare_first_line(), VARIANT_LABEL_MAX_CHARS)
}

fn gauge_vec(
    registry: &Registry,
    name: &str,
    help: &str,
    labels: &[&str],
) -> prometheus::Result<GaugeVec> {
    let gauge = GaugeVec::new(Opts::new(name, help), labels)?;
    registry.register(Box::new(gauge.clone()))?;
    Ok(gauge)
}

fn truncate(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

/// Latest rendered exposition text, swapped in whole by each [`Metrics::refresh`].
#[derive(Default)]
pub struct Metrics {
    body: RwLock<String>,
}

impl Metrics {
    async fn refresh(&self, pool: &SqlitePool) -> anyhow::Result<()> {
        let body = IdentityCounts::collect(pool).await?.render()?;
        *self.body.write().unwrap() = body;
        Ok(())
    }

    fn body(&self) -> String {
        self.body.read().unwrap().clone()
    }
}

async fn scrape(State(metrics): State<Arc<Metrics>>) -> impl IntoResponse {
    (
        [(
            header::CONTENT_TYPE,
            TextEncoder::new().format_type().to_owned(),
        )],
        metrics.body(),
    )
}

/// Serves `GET /metrics` on the configured address, and keeps the gauges it exposes fresh
/// from a background task. Runs the first refresh before accepting scrapes, so an early
/// scrape never sees an empty body.
pub async fn serve(config: MetricsConfig, pool: SqlitePool) -> anyhow::Result<()> {
    let metrics = Arc::new(Metrics::default());
    metrics.refresh(&pool).await?;

    let period = Duration::from_secs(config.refresh_seconds.max(1));
    let refresher = metrics.clone();

    tokio::spawn(async move {
        let mut ticker = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
        loop {
            ticker.tick().await;
            if let Err(err) = refresher.refresh(&pool).await {
                tracing::warn!(error = ?err, "Could not refresh metrics, serving previous values");
            }
        }
    });

    let router = Router::new()
        .route("/metrics", get(scrape))
        .with_state(metrics);
    let listener = tokio::net::TcpListener::bind(&config.listen).await?;

    tracing::info!("Metrics listening on {}", listener.local_addr()?);
    axum::serve(listener, router).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn variant(
        feature: &str,
        variant_id: i32,
        value: &str,
        identities: i64,
    ) -> VariantIdentityCount {
        VariantIdentityCount {
            project: "demo".into(),
            environment: "dev".into(),
            feature: feature.into(),
            variant_id,
            variant: VariantValue::build(value),
            identities,
        }
    }

    #[test]
    fn renders_baseline_series() {
        let counts = IdentityCounts {
            totals: vec![IdentityTotal {
                project: "demo".into(),
                environment: "dev".into(),
                identities: 10,
            }],
            variants: vec![
                variant("checkout", 1, "on", 3),
                variant("checkout", 2, "off", 1),
            ],
            traits: vec![TraitIdentityCount {
                project: "demo".into(),
                environment: "dev".into(),
                trait_name: "country".into(),
                identities: 7,
            }],
            ..Default::default()
        };
        let out = counts.render().unwrap();

        assert!(out.contains(r#"flagrant_identities_total{environment="dev",project="demo"} 10"#));
        assert!(out.contains(
            r#"flagrant_variant_identities{environment="dev",feature="checkout",project="demo",variant="on",variant_id="1"} 3"#
        ));
        assert!(out.contains(
            r#"flagrant_variant_identities_ratio{environment="dev",feature="checkout",project="demo",variant="on",variant_id="1"} 0.75"#
        ));
        assert!(out.contains(
            r#"flagrant_variant_identities_ratio{environment="dev",feature="checkout",project="demo",variant="off",variant_id="2"} 0.25"#
        ));
        assert!(out.contains(
            r#"flagrant_trait_identities{environment="dev",project="demo",trait="country"} 7"#
        ));
    }

    #[test]
    fn ratio_is_zero_when_nobody_is_assigned() {
        let counts = IdentityCounts {
            variants: vec![
                variant("checkout", 1, "on", 0),
                variant("checkout", 2, "off", 0),
            ],
            ..Default::default()
        };
        let out = counts.render().unwrap();

        assert!(!out.contains("NaN"));
        assert_eq!(out.matches("} 0\n").count(), 4);
    }

    #[test]
    fn ratios_are_scoped_per_feature() {
        let counts = IdentityCounts {
            variants: vec![variant("a", 1, "x", 1), variant("b", 2, "y", 4)],
            ..Default::default()
        };
        let out = counts.render().unwrap();

        // each feature is its own denominator, so a lone variant owns its whole feature
        assert!(out.contains(r#"flagrant_variant_identities_ratio{environment="dev",feature="a",project="demo",variant="x",variant_id="1"} 1"#));
        assert!(out.contains(r#"flagrant_variant_identities_ratio{environment="dev",feature="b",project="demo",variant="y",variant_id="2"} 1"#));
    }

    #[test]
    fn long_variant_values_are_truncated_in_the_label() {
        let counts = IdentityCounts {
            variants: vec![variant("f", 1, &"x".repeat(500), 1)],
            ..Default::default()
        };
        let out = counts.render().unwrap();

        assert!(out.contains(&format!(
            r#"variant="{}""#,
            "x".repeat(VARIANT_LABEL_MAX_CHARS)
        )));
        assert!(!out.contains(&"x".repeat(VARIANT_LABEL_MAX_CHARS + 1)));
    }

    fn segment_variant(
        segment: &str,
        variant_id: i32,
        value: &str,
        identities: i64,
    ) -> SegmentVariantIdentityCount {
        SegmentVariantIdentityCount {
            project: "demo".into(),
            environment: "dev".into(),
            feature: "checkout".into(),
            segment: segment.into(),
            variant_id,
            variant: VariantValue::build(value),
            identities,
        }
    }

    #[test]
    fn renders_segment_series() {
        let counts = IdentityCounts {
            segment_variants: vec![
                segment_variant("vip", 1, "old", 1),
                segment_variant("vip", 2, "new", 4),
                segment_variant("beta", 1, "old", 0),
                segment_variant("beta", 2, "new", 0),
            ],
            dirty: vec![DirtyIdentityCount {
                project: "demo".into(),
                environment: "dev".into(),
                feature: "checkout".into(),
                identities: 3,
            }],
            ..Default::default()
        };
        let out = counts.render().unwrap();

        assert!(out.contains(
            r#"flagrant_segment_variant_identities{environment="dev",feature="checkout",project="demo",segment="vip",variant="new",variant_id="2"} 4"#
        ));
        // a segment's total is the sum of its variants, and one without identities is still reported
        assert!(out.contains(
            r#"flagrant_segment_identities{environment="dev",feature="checkout",project="demo",segment="vip"} 5"#
        ));
        assert!(out.contains(
            r#"flagrant_segment_identities{environment="dev",feature="checkout",project="demo",segment="beta"} 0"#
        ));
        assert!(out.contains(
            r#"flagrant_segment_dirty_identities{environment="dev",feature="checkout",project="demo"} 3"#
        ));
    }
}
