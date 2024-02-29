#![allow(dead_code)]

use flagrant::Feature;
use std::collections::{HashMap, HashSet};

#[test]
fn test_variadic_weights_eq_100_percent() {
    assert!(Feature::new_variadic(
        String::from("sample feature"),
        String::from("controlle value"),
        true,
        vec![(String::from("Big"), 40), (String::from("Small"), 20)]
    )
    .is_ok())
}

#[test]
fn test_variadic_weights_gt_100_percent() {
    assert!(Feature::new_variadic(
        String::from("sample feature"),
        String::from("control value"),
        true,
        vec![(String::from("Big"), 90), (String::from("Small"), 30)]
    )
    .is_err())
}

#[test]
fn test_variadic_buckets() {
    let mut feature = Feature::new_variadic(
        String::from("sample feature"),
        String::from("control value"),
        true,
        vec![(String::from("Big"), 10), (String::from("Small"), 30)],
    )
    .unwrap();

    let mut buckets = HashMap::<String, usize>::new();

    for _ in 1..=100 {
        if let Ok((id, _)) = feature.get_value(None) {
            *buckets.entry(id.unwrap().to_string()).or_insert(0) += 1;
        }
    }
    let values = buckets.values().cloned().collect::<HashSet<_>>();

    assert!(values.contains(&10));
    assert!(values.contains(&30));
    assert!(values.contains(&60));
}
