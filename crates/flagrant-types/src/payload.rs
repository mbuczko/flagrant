use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{Environment, Feature, FeatureValue, Project, TraitValue};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub enum VariantPatchOp {
    Add { value: FeatureValue, weight: u8 },
    SetValue { id: i32, value: FeatureValue },
    SetWeight { id: i32, weight: u8 },
    Delete { id: i32 },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct FeaturePatch {
    pub is_enabled: Option<bool>,
    pub is_active: Option<bool>,
    pub variants: Vec<VariantPatchOp>,
}

impl FeaturePatch {
    pub fn is_empty(&self) -> bool {
        self.is_enabled.is_none() && self.is_active.is_none() && self.variants.is_empty()
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProjectRequestPayload {
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProjectCreatedResponse {
    pub project: Project,
    pub environment: Environment,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct NewEnvironmentPayload {
    pub name: String,
    pub description: Option<String>,
    pub base_env: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct NewFeaturePayload {
    pub name: String,
    pub value: FeatureValue,
    pub description: Option<String>,
    pub is_enabled: bool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct VariantRequestPayload {
    pub value: String,
    pub weight: u8,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TraitRequestPayload {
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct IdentityTraitPayload {
    pub name: String,
    pub value: Option<TraitValue>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct IdentityRequestPayload {
    pub identity: String,
    pub traits: Option<Vec<IdentityTraitPayload>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub enum TraitPatchOp {
    Add { name: String, value: Option<TraitValue> },
    Delete { name: String },
    SetValue { name: String, value: Option<TraitValue> },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct IdentityPatch {
    pub identity: Option<String>,
    pub traits: Vec<TraitPatchOp>,
}

impl IdentityPatch {
    pub fn is_empty(&self) -> bool {
        self.identity.is_none() && self.traits.is_empty()
    }
}

impl From<Feature> for NewFeaturePayload {
    fn from(feature: Feature) -> Self {
        NewFeaturePayload {
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
