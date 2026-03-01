use serde::{Deserialize, Serialize};

use crate::{Feature, FeatureValue};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VariantPatchOp {
    Add { value: String, weight: u8 },
    SetValue { id: i32, value: String },
    SetWeight { id: i32, weight: u8 },
    Delete { id: i32 },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FeaturePatch {
    pub is_enabled: Option<bool>,
    pub is_active: Option<bool>,
    pub value: Option<FeatureValue>,
    pub variants: Vec<VariantPatchOp>,
}

impl FeaturePatch {
    pub fn is_empty(&self) -> bool {
        self.is_enabled.is_none()
            && self.is_active.is_none()
            && self.value.is_none()
            && self.variants.is_empty()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EnvRequestPayload {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FeatureRequestPayload {
    pub name: String,
    pub value: FeatureValue,
    pub description: Option<String>,
    pub is_enabled: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VariantRequestPayload {
    pub value: String,
    pub weight: u8,
}

impl From<Feature> for FeatureRequestPayload {
    fn from(feature: Feature) -> Self {
        FeatureRequestPayload {
            name: feature.name,
            value: feature
                .variants
                .into_iter()
                .find(|v| v.environment_id.is_some())
                .expect("Feature has no control variant!")
                .value,
            description: None,
            is_enabled: feature.is_enabled,
        }
    }
}
