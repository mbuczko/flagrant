use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, RwLock},
    thread,
};

use rand::{rngs::ThreadRng, Rng};
use ulid::Ulid;

const REQUESTS_COUNT: usize = 1000;
const THREADS_COUNT: usize = 8;

type LockIdents = Arc<RwLock<Vec<Ulid>>>;
type LockResults = Arc<RwLock<BTreeMap<String, BTreeSet<String>>>>;

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

                        // check if user's ID isn't already assigned to other value.
                        check_and_evict(&results_clone, &id);

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

/// Generates new or retrieves already existing user's ULID turned into a String.
/// For performance reasons, provided `idents` vector is read-locked first to check
/// if ULID already exists. This becomes the most common case after REQUESTS_COUNT
/// number of requests gets reached.
///
/// If no ULID was found, vector is write-relocked for mutation - adding new ULID.
/// This way either newly created ULID or existing one is being returned without
/// excessive locking.
fn get_or_generate_id(idents: &LockIdents, rng: &mut ThreadRng) -> Option<String> {
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

fn check_and_evict(results: &LockResults, id: &String) {
    if let Ok(results_read) = results.read() {
        for key in results_read.keys() {
            if results_read.get(key).unwrap().contains(id) {
                // drop read-lock and re-enter with read-write lock
                let key_clone = key.clone();
                drop(results_read);

                if let Ok(mut results_write) = results.write() {
                    let idents = results_write.get_mut(&key_clone).unwrap();
                    idents.remove(id);
                    if idents.is_empty() {
                        results_write.remove(&key_clone);
                    }
                }
                break;
            }
        }
    }
}
fn get_flag_value(_ident: &str) -> String {
    String::from("dupa")
}
