use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

use argh::FromArgs;
use flagrant_client::{connection::Connection, http::Auth};
use flagrant_types::{Feature, FeatureResponse, FeatureValue};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use rand::{Rng, rngs::ThreadRng};
use ulid::Ulid;

#[derive(FromArgs)]
/// Flagrant load-testing bombardier
struct Args {
    /// API host (default: http://localhost:3030)
    #[argh(
        option,
        short = 'h',
        default = "String::from(\"http://localhost:3030\")"
    )]
    host: String,

    /// project name (default: sample)
    #[argh(option, short = 'p', default = "String::from(\"sample\")")]
    project: String,

    /// environment name (or id)
    #[argh(option, short = 'e')]
    environment: String,

    /// feature name
    #[argh(option, short = 'f')]
    feature: String,

    /// number of synthetic identities to distribute across variants (default: 100)
    #[argh(option, short = 'n', default = "100")]
    idents: usize,

    /// number of additional named identities (tester-1, tester-2, ...) seeded and printed
    /// up front, for manually testing segment/identity overrides via the CLI (default: 10)
    #[argh(option, short = 'm', default = "10")]
    named_idents: usize,

    /// number of worker threads polling the API concurrently (default: 1)
    #[argh(option, short = 't', default = "1")]
    threads: usize,
}

static IDX: AtomicUsize = AtomicUsize::new(0);

fn feature_value(response: Vec<FeatureResponse>, feature_name: &str) -> Option<FeatureValue> {
    response
        .into_iter()
        .find(|r| r.name == feature_name)
        .map(|f| f.value)
}

pub fn main() -> anyhow::Result<()> {
    let args: Args = argh::from_env();
    let idents_count = args.idents + args.named_idents;

    let idents = Arc::new(RwLock::new(HashMap::<usize, String>::with_capacity(
        idents_count,
    )));

    // Seed a handful of easy-to-type identities (on top of the random pool below) so they
    // can be referenced directly from the CLI to set up segment/identity overrides.
    let named: Vec<String> = (1..=args.named_idents)
        .map(|n| format!("tester-{n}"))
        .collect();
    {
        let mut guard = idents.write().unwrap();
        for (i, name) in named.iter().enumerate() {
            guard.insert(i, name.clone());
        }
    }
    IDX.store(args.named_idents, Ordering::SeqCst);

    println!(
        "Seeded {} named identities for manual testing: {}",
        named.len(),
        named.join(", ")
    );
    println!();

    let buckets = Arc::new(RwLock::new(HashMap::new()));
    let connection = Arc::new(Connection::init(
        args.host,
        Auth::None,
        args.project,
        args.environment,
    )?);

    // Flipped to `false` by the watcher thread below only on an unrecoverable error (the
    // feature got deleted, the API is unreachable, ...), so every loop (workers, the watcher
    // itself, and the progress renderer) winds down and `main` can report why the run stopped.
    let running = Arc::new(AtomicBool::new(true));

    // Tracks whether the feature is currently enabled. Starts `false` so workers wait for the
    // watcher's first check rather than assuming enabled; toggled as the feature is enabled/
    // disabled over the run instead of ending the whole process on a disabled feature.
    let enabled = Arc::new(AtomicBool::new(false));

    thread::scope(|s| {
        for _ in 0..args.threads {
            let idents = Arc::clone(&idents);
            let buckets = Arc::clone(&buckets);
            let conn = Arc::clone(&connection);
            let running = Arc::clone(&running);
            let enabled = Arc::clone(&enabled);
            let feature_name = args.feature.as_str();

            s.spawn(move || {
                let mut rng = rand::thread_rng();
                while running.load(Ordering::Relaxed) {
                    if !enabled.load(Ordering::Relaxed) {
                        thread::sleep(Duration::from_millis(200));
                        continue;
                    }
                    // TODO: fetch idents_count idents from the pool and generate new ones if needed
                    if let Some(ident) = get_or_generate_ident(&idents, idents_count, &mut rng)
                        && let Some(response) = conn.get_features(&ident)
                        && let Some(fv) = feature_value(response, feature_name)
                    {
                        let mut guard = buckets.write().unwrap();
                        let val = match fv {
                            FeatureValue::Json(v) => v,
                            FeatureValue::Toml(v) => v,
                            FeatureValue::Text(v) => v,
                        };
                        // Evict ident from all buckets
                        evict_from_buckets(&mut guard, &ident);

                        // Add value to corresponding bucket
                        guard.entry(val).or_insert_with(HashSet::new).insert(ident);

                        std::mem::drop(guard);
                        thread::sleep(Duration::from_millis(50));
                    }
                }
            });
        }

        #[allow(clippy::literal_string_with_formatting_args)]
        let sty = ProgressStyle::with_template("[{pos:>7}/{len:7}] {bar:40.cyan/blue} {msg}")
            .unwrap()
            .progress_chars("##-");

        let m = MultiProgress::new();

        // Re-fetches the feature every couple seconds for the lifetime of the run - not just
        // once at startup - so bombardier reacts as soon as someone enables/disables the
        // feature mid-run, instead of only ever checking before the first request.
        {
            let conn = Arc::clone(&connection);
            let running = Arc::clone(&running);
            let enabled = Arc::clone(&enabled);
            let m = m.clone();
            let feature_name = args.feature.as_str();
            let feature_path = conn
                .env_resource()
                .subpath(format!("/features/{feature_name}"));

            s.spawn(move || {
                let mut last_enabled = None;
                while running.load(Ordering::Relaxed) {
                    match conn.client.get::<Feature>(feature_path.clone()) {
                        Ok(feature) => {
                            if last_enabled != Some(feature.is_enabled) {
                                let msg = if feature.is_enabled {
                                    format!("Feature '{feature_name}' is enabled - distributing.")
                                } else {
                                    format!(
                                        "Feature '{feature_name}' is disabled - waiting for it to be enabled (e.g. via the CLI)..."
                                    )
                                };
                                let _ = m.println(msg);
                                last_enabled = Some(feature.is_enabled);
                            }
                            enabled.store(feature.is_enabled, Ordering::Relaxed);
                            thread::sleep(Duration::from_secs(2));
                        }
                        Err(err) => {
                            let _ =
                                m.println(format!("Could not fetch feature '{feature_name}': {err}"));
                            running.store(false, Ordering::Relaxed);
                        }
                    }
                }
            });
        }

        let mut pbs = HashMap::<String, ProgressBar>::new();
        while running.load(Ordering::Relaxed) {
            let guard = buckets.read().unwrap();
            for (val, set) in guard.iter() {
                if set.is_empty() {
                    if let Some(pb) = pbs.remove(val) {
                        m.remove(&pb);
                    }
                } else {
                    let pb = pbs.entry(val.clone()).or_insert_with(|| {
                        let pb = ProgressBar::new(idents_count as u64);
                        pb.set_style(sty.clone());
                        m.add(pb)
                    });
                    pb.set_message(format!(
                        "{} ({}%)",
                        val,
                        ((100 * set.len()) / idents_count) as u64
                    ));
                    pb.set_position(set.len() as u64);
                }
            }
        }
    });

    if !running.load(Ordering::Relaxed) {
        anyhow::bail!(
            "Stopped: feature '{}' is no longer available.",
            args.feature
        );
    }
    Ok(())
}

fn get_or_generate_ident(
    idents: &Arc<RwLock<HashMap<usize, String>>>,
    idents_count: usize,
    rng: &mut ThreadRng,
) -> Option<String> {
    let mut guard = idents.write().unwrap();
    if guard.len() >= idents_count {
        return guard.get(&rng.gen_range(0..idents_count)).cloned();
    }
    let ulid = Ulid::new().to_string();
    let idx = IDX.fetch_add(1, Ordering::SeqCst);

    guard.insert(idx, ulid.clone());
    Some(ulid)
}

fn evict_from_buckets(buckets: &mut HashMap<String, HashSet<String>>, ident: &str) {
    for v in buckets.values_mut() {
        v.remove(ident);
    }
}
