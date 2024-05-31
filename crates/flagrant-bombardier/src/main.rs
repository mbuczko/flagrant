use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, RwLock},
    thread,
};

use rand::{rngs::ThreadRng, Rng};
use ulid::Ulid;

const REQUESTS_COUNT: usize = 1000;
const THREADS_COUNT: usize = 8;

pub fn main() -> anyhow::Result<()> {
    let idents = Arc::new(RwLock::new(Vec::<Ulid>::with_capacity(REQUESTS_COUNT)));
    let results = Arc::new(RwLock::new(BTreeMap::<String, BTreeSet<String>>::new()));

    thread::scope(|s| {
        for _ in 1..=THREADS_COUNT {
            let idents_clone = idents.clone();
            let results_clone = results.clone();

            s.spawn(move || {
                let mut rng = rand::thread_rng();
                loop {
                    let id = get_or_generate_id(&idents_clone, &mut rng);
                    if let Some(id) = id {
                        let val = get_flag_value(&id);

                        // add value to corresponding bucket
                        if let Ok(mut res) = results_clone.write() {
                            res.entry(val).or_insert(BTreeSet::new()).insert(id);
                        }
                    }
                }
            });
        }
    });
    Ok(())
}

/// Generates new or retrieves already created id and returns it as stringified Option.
/// Locks input `idents` Vec with read-lock first as this is the most common case after
/// REQUESTS_COUNT requests being fired.
fn get_or_generate_id(idents: &Arc<RwLock<Vec<Ulid>>>, rng: &mut ThreadRng) -> Option<String> {
    if let Ok(idents_read) = idents.read() {
        if idents_read.len() >= REQUESTS_COUNT {
            return idents_read.get(rng.gen_range(0..REQUESTS_COUNT)).map(|id| id.to_string());
        } else {
            // drop read-lock and re-enter with read-write lock
            drop(idents_read);
            if let Ok(mut idents) = idents.write() {
                idents.push(ulid::Ulid::new());
                return idents.last().map(|id| id.to_string());
            }
        }
    }
    None
}

fn get_flag_value(_ident: &str) -> String {
    String::from("dupa")
}
