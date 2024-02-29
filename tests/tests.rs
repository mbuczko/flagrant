#![allow(dead_code)]

use flagrant::{distributor::Variation, Feature, FeatureValue};
use std::collections::{HashMap, HashSet};

#[test]
fn simple_feature_value() {
    let value = String::from("control value");
    let mut feature = Feature::new(String::from("sample feature"), value.clone()).unwrap();

    // simple feature - no variations, just a constant value
    assert!(feature.variations().is_err());
    assert_eq!(feature.value(None), FeatureValue::Simple(&value));
}

#[test]
fn multivariadic_feature_with_control_value_only() {
    let control_value = String::from("control value");
    let mut feature = Feature::new_variadic(
        String::from("sample feature"),
        control_value.clone(),
        vec![],
    )
    .unwrap();

    let variations = feature.variations().unwrap();
    assert_eq!(variations.len(), 1);

    let id = variations.first().unwrap().id;
    assert_eq!(
        feature.value(None),
        FeatureValue::Variadic(&Variation {
            id,
            value: control_value,
            weight: 100
        })
    );
}

#[test]
fn multivariadic_feature_with_some_more_variations() {
    let variations = vec![
        (String::from("First alternative"), 30),
        (String::from("Second alternative"), 50),
    ];
    let feature = Feature::new_variadic(
        String::from("sample feature"),
        String::from("control value"),
        variations,
    )
    .unwrap();

    let variations = feature.variations().unwrap();
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
    let mut buckets = HashMap::<String, usize>::new();
    let mut feature = Feature::new_variadic(
        String::from("sample feature"),
        String::from("control value"),
        vec![],
    )
    .unwrap();

    for _ in 1..=100 {
        if let Some(variation) = feature.variation(None) {
            *buckets.entry(variation.id.to_string()).or_insert(0) += 1;
        }
    }

    let hits = buckets.values().collect::<HashSet<_>>();
    assert!(hits.len() == 1);
    assert!(hits.contains(&100));
}

#[test]
fn variadic_distribution_with_more_variations() {
    let mut buckets = HashMap::<String, usize>::new();
    let mut feature = Feature::new_variadic(
        String::from("sample feature"),
        String::from("control value"),
        vec![(String::from("Big"), 10), (String::from("Small"), 30)],
    )
    .unwrap();

    for _ in 1..=100 {
        if let Some(variation) = feature.variation(None) {
            *buckets.entry(variation.id.to_string()).or_insert(0) += 1;
        }
    }

    let hits = buckets.values().collect::<HashSet<_>>();
    assert!(hits.contains(&10));
    assert!(hits.contains(&30));
    assert!(hits.contains(&60));
}
