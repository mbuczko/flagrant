use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, RwLock,
    },
    thread,
    time::Duration,
};

use flagrant_client::{http::Auth, session::Session};
use flagrant_types::{FeatureResponse, FeatureValue};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use rand::{rngs::ThreadRng, Rng};
use ulid::Ulid;

const IDENTS_COUNT: usize = 100;
const THREADS_COUNT: usize = 1;
const PROJECT_ID: i32 = 1;
const ENVIRONMENT_ID: i32 = 1;
const FEATURE_ID: i32 = 1;

static IDX: AtomicUsize = AtomicUsize::new(0);

fn feature_value(response: Vec<FeatureResponse>, feature_id: i32) -> Option<FeatureValue> {
    response
        .into_iter()
        .find(|r| r.feature_id == feature_id)
        .map(|f| f.value)
}

pub fn main() -> anyhow::Result<()> {
    let idents = Arc::new(RwLock::new(HashMap::<usize, Ulid>::with_capacity(
        IDENTS_COUNT,
    )));
    let buckets = Arc::new(RwLock::new(HashMap::new()));
    let session = Arc::new(Session::init(
        "http://localhost:3030".into(),
        Auth::None,
        PROJECT_ID,
        ENVIRONMENT_ID,
    )?);

    thread::scope(|s| {
        for _ in 0..THREADS_COUNT {
            let idents = Arc::clone(&idents);
            let buckets = Arc::clone(&buckets);
            let session = Arc::clone(&session);

            s.spawn(move || {
                let mut rng = rand::thread_rng();
                loop {
                    if let Some(ident) = get_or_generate_ident(&idents, &mut rng) {
                        if let Some(response) = session.get_features(&ident) {
                            if let Some(fv) = feature_value(response, FEATURE_ID) {
                                let mut guard = buckets.write().unwrap();
                                let val = match fv {
                                    FeatureValue::Json(v) => v,
                                    FeatureValue::Toml(v) => v,
                                    FeatureValue::Text(v) => v,
                                };
                                // evict ident from all buckets
                                evict_from_buckets(&mut guard, &ident);

                                // add value to corresponding bucket
                                guard.entry(val).or_insert_with(HashSet::new).insert(ident);

                                std::mem::drop(guard);
                                thread::sleep(Duration::from_millis(50));
                            }
                        }
                    }
                }
            });
        }

        let m = MultiProgress::new();
        let sty = ProgressStyle::with_template("[{pos:>7}/{len:7}] {bar:40.cyan/blue} {msg}")
            .unwrap()
            .progress_chars("##-");

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
                        let pb = ProgressBar::new(IDENTS_COUNT as u64);
                        pb.set_style(sty.clone());
                        m.add(pb)
                    });
                    pb.set_message(format!(
                        "{} ({}%)",
                        &val,
                        ((100 * set.len()) / IDENTS_COUNT) as u64
                    ));
                    pb.set_position(set.len() as u64);
                }
            }
        }
    });
    Ok(())
}

fn get_or_generate_ident(
    idents: &Arc<RwLock<HashMap<usize, Ulid>>>,
    rng: &mut ThreadRng,
) -> Option<String> {
    let mut guard = idents.write().unwrap();
    if guard.len() >= IDENTS_COUNT {
        return guard
            .get(&rng.gen_range(0..IDENTS_COUNT))
            .map(|id| id.to_string());
    }
    let ulid = ulid::Ulid::new();

    guard.insert(IDX.fetch_add(1, Ordering::SeqCst), ulid);
    Some(ulid.to_string())
}

fn evict_from_buckets(buckets: &mut HashMap<String, HashSet<String>>, ident: &str) {
    for (_, v) in buckets.iter_mut() {
            v.remove(ident);
    }
}
