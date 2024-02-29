#![allow(dead_code)]

use flagrant::Feature;
use std::collections::{HashMap, HashSet};

#[test]
fn simple_feature_value() {
    let value = String::from("control value");
    let mut feature =
        Feature::new(String::from("sample feature"), value.clone()).unwrap();

    assert!(feature.get_variations().is_err());
    assert_eq!(feature.get_value(None).1, &value);
}

#[test]
fn multivariadic_feature_with_control_value_only() {
    let feature = Feature::new_variadic(
        String::from("sample feature"),
        String::from("control value"),
        vec![],
    )
    .unwrap();

    let variations = feature.get_variations().unwrap();
    assert_eq!(variations.len(), 1);
    assert_eq!(variations.first().unwrap().weight, 100);
}

#[test]
fn multivariadic_feature_with_some_more_variations() {
    let feature = Feature::new_variadic(
        String::from("sample feature"),
        String::from("control value"),
        vec![
            (String::from("First alternative"), 30),
            (String::from("Second alternative"), 50)
        ],
    )
    .unwrap();

    let variations = feature.get_variations().unwrap();
    assert_eq!(variations.len(), 3);
    assert_eq!(variations.first().unwrap().weight, 30);
    assert_eq!(variations.get(1).unwrap().weight, 50);
    // a control value with remaining weight
    assert_eq!(variations.get(2).unwrap().weight, 20);
}

#[test]
fn variadic_weights_exceed_100_percent() {
    assert!(Feature::new_variadic(
        String::from("sample feature"),
        String::from("control value"),
        vec![(String::from("Big"), 90), (String::from("Small"), 30)]
    )
    .is_err())
}

#[test]
fn variadic_distribution_with_one_variation() {
    let mut feature = Feature::new_variadic(
        String::from("sample feature"),
        String::from("control value"),
        vec![],
    )
    .unwrap();

    let mut buckets = HashMap::<String, usize>::new();

    for _ in 1..=100 {
        let (id, _) = feature.get_value(None);
        *buckets.entry(id.unwrap().to_string()).or_insert(0) += 1;
    }
    assert!(buckets.values().collect::<HashSet<_>>().contains(&100));
}

#[test]
fn variadic_distribution_with_more_variations() {
    let mut feature = Feature::new_variadic(
        String::from("sample feature"),
        String::from("control value"),
        vec![(String::from("Big"), 10), (String::from("Small"), 30)],
    )
    .unwrap();

    let mut buckets = HashMap::<String, usize>::new();

    for _ in 1..=100 {
        let (id, _) = feature.get_value(None);
        *buckets.entry(id.unwrap().to_string()).or_insert(0) += 1;
    }

    let values = buckets.values().collect::<HashSet<_>>();
    assert!(values.contains(&10));
    assert!(values.contains(&30));
    assert!(values.contains(&60));
}
