use std::{sync::{atomic::{AtomicUsize, Ordering}, Arc}, thread};

use dashmap::{DashMap, DashSet};
use flagrant_client::session::Session;
use rand::{rngs::ThreadRng, Rng};
use ulid::Ulid;

const REQUESTS_COUNT: usize = 1000;
const THREADS_COUNT: usize = 8;

static IDX: AtomicUsize = AtomicUsize::new(0);

pub fn main() -> anyhow::Result<()> {
    let idents = Arc::new(DashMap::<usize, Ulid>::with_capacity(REQUESTS_COUNT));
    let results = Arc::new(DashMap::<String, DashSet<String>>::new());

    let _session = Session::init("http://localhost:3030".into(), 1, 1)?;

    thread::scope(|s| {
        for _ in 1..=THREADS_COUNT {
            let idents = Arc::clone(&idents);
            let results = Arc::clone(&results);

            s.spawn(move || {
                let mut rng = rand::thread_rng();
                loop {
                    let id = get_or_generate_id(idents.clone(), &mut rng);
                    if let Some(id) = id {
                        let val = get_flag_value(&id);

                        // check if user's ID isn't already assigned to other value.
                        evict_from_buckets(results.clone(), &id);

                        // add value to corresponding bucket
                        results.entry(val).or_default().insert(id);
                    }
                }
            });
        }
    });
    Ok(())
}

fn get_or_generate_id(idents: Arc<DashMap<usize, Ulid>>, rng: &mut ThreadRng) -> Option<String> {
    if idents.len() >= REQUESTS_COUNT {
        return idents
            .get(&rng.gen_range(0..REQUESTS_COUNT))
            .map(|id| id.to_string());
    }
    idents
        .insert(IDX.fetch_add(1, Ordering::SeqCst), ulid::Ulid::new())
        .map(|id| id.to_string())
}

fn evict_from_buckets(results: Arc<DashMap<String, DashSet<String>>>, id: &String) {
    for entry in results.iter() {
        entry.value().remove(id);
    }
}

fn get_flag_value(_ident: &str) -> String {
    String::from("dupa")
}
