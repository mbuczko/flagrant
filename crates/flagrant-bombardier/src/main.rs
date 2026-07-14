use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, RwLock,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

use argh::FromArgs;
use flagrant_client::{connection::Connection, http::Auth};
use flagrant_types::{FeatureResponse, FeatureValue};
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

    println!("Seeded {} named identities for manual testing:", named.len());
    for name in &named {
        println!("  {name}");
    }
    println!();

    let buckets = Arc::new(RwLock::new(HashMap::new()));
    let connection = Arc::new(Connection::init(
        args.host,
        Auth::None,
        args.project,
        args.environment,
    )?);

    thread::scope(|s| {
        for _ in 0..args.threads {
            let idents = Arc::clone(&idents);
            let buckets = Arc::clone(&buckets);
            let conn = Arc::clone(&connection);
            let feature_name = args.feature.as_str();

            s.spawn(move || {
                let mut rng = rand::thread_rng();
                loop {
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
        let mut pbs = HashMap::<String, ProgressBar>::new();
        loop {
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
