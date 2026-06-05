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

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub enum TraitPatchOp {
    Add {
        name: String,
        value: Option<TraitValue>,
    },
    Delete {
        name: String,
    },
    SetValue {
        name: String,
        value: Option<TraitValue>,
    },
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct NewProjectPayload {
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
pub struct NewVariantPayload {
    pub value: String,
    pub weight: u8,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct NewIdentityPayload {
    pub identity: String,
    pub traits: Option<Vec<IdentityTraitPayload>>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct NewTraitPayload {
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct IdentityTraitPayload {
    pub name: String,
    pub value: Option<TraitValue>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct FeaturePatch {
    pub is_enabled: Option<bool>,
    pub is_archived: Option<bool>,
    pub variants: Vec<VariantPatchOp>,
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

impl FeaturePatch {
    pub fn is_empty(&self) -> bool {
        self.is_enabled.is_none() && self.is_archived.is_none() && self.variants.is_empty()
    }
}

/// A single staged variant override for one feature, carried inside [`IdentityPatch`].
/// The server resolves feature name to an existing feature and value to a variant.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IdentityOverridePatch {
    pub feature_name: String,
    pub variant_value: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct IdentityPatch {
    pub traits: Vec<TraitPatchOp>,
    pub overrides: Vec<IdentityOverridePatch>,
}

impl IdentityPatch {
    pub fn is_empty(&self) -> bool {
        self.traits.is_empty() && self.overrides.is_empty()
    }
}
