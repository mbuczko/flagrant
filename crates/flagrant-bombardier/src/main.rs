use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    thread,
};

use dashmap::{DashMap, DashSet};
use flagrant_client::session::Session;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use rand::{rngs::ThreadRng, Rng};
use ulid::Ulid;

const IDENTS_COUNT: usize = 1000;
const THREADS_COUNT: usize = 8;

static IDX: AtomicUsize = AtomicUsize::new(0);

pub fn main() -> anyhow::Result<()> {
    let idents = Arc::new(DashMap::<usize, Ulid>::with_capacity(IDENTS_COUNT));
    let results = Arc::new(DashMap::<String, DashSet<String>>::new());
    let session = Arc::new(Session::init("http://localhost:3030".into(), 1, 1)?);

    thread::scope(|s| {
        for _ in 1..=THREADS_COUNT {
            let idents = Arc::clone(&idents);
            let results = Arc::clone(&results);
            let session = Arc::clone(&session);

            s.spawn(move || {
                let mut rng = rand::thread_rng();
                loop {
                    if let Some(id) = get_or_generate_id(Arc::clone(&idents), &mut rng) {
                        if let Some(fv) = session.get_feature(&id, "spookie") {
                            let value = fv.0;

                            // check if user's ID isn't already assigned to other value.
                            evict_from_buckets(Arc::clone(&results), &value, &id);

                            // add value to corresponding bucket
                            results.entry(value).or_default().insert(id);
                        }
                    }
                }
            });
        }

        let m = MultiProgress::new();
        let sty = ProgressStyle::with_template(
            "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}",
        )
        .unwrap()
        .progress_chars("##-");

        let pbs = DashMap::<String, ProgressBar>::new();
        loop {
            for res in results.iter() {
                let (val, set) = res.pair();
                if set.is_empty() {
                    if let Some((_, pb)) = pbs.remove(val) {
                        m.remove(&pb);
                    }
                } else {
                    let mut pb = pbs.entry(val.clone()).or_insert_with(|| {
                        let pb = ProgressBar::new(IDENTS_COUNT as u64);
                        pb.set_style(sty.clone());
                        m.add(pb)
                    });
                    pb.set_message(format!(
                        "{} ({}%)",
                        val.clone(),
                        ((100 * set.len()) / IDENTS_COUNT) as u64
                    ));
                    pb.value_mut().set_position(set.len() as u64);
                }
            }
        }
    });
    Ok(())
}

fn get_or_generate_id(idents: Arc<DashMap<usize, Ulid>>, rng: &mut ThreadRng) -> Option<String> {
    if idents.len() >= IDENTS_COUNT {
        return idents
            .get(&rng.gen_range(0..IDENTS_COUNT))
            .map(|id| id.to_string());
    }
    idents
        .insert(IDX.fetch_add(1, Ordering::SeqCst), ulid::Ulid::new())
        .map(|id| id.to_string())
}

fn evict_from_buckets(results: Arc<DashMap<String, DashSet<String>>>, target: &String, id: &String) {
    for entry in results.iter() {
        if entry.key() != target {
            entry.value().remove(id);
        }
    }
}
