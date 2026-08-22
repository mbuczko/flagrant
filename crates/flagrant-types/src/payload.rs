use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use utoipa::ToSchema;

use crate::{
    Comparator, Environment, Feature, GroupConnector, IdentityWithTraits, Project, RolloutConfig,
    Segment, Snapshot, Subject, TraitValue, VariantValue,
};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub enum VariantPatchOp {
    Add { value: VariantValue, weight: u8 },
    SetValue { id: i32, value: VariantValue },
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

#[derive(Debug, Clone, Serialize, Deserialize, Validate, ToSchema)]
pub enum TagPatchOp {
    Add(
        #[validate(pattern = r"^[A-Za-z0-9][A-Za-z0-9+_.-]*$")]
        #[validate(max_length = 32)]
        String,
    ),
    Remove(String),
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
pub struct UpdateEnvironmentPayload {
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct NewFeaturePayload {
    pub name: String,
    pub value: VariantValue,
    pub description: Option<String>,
    pub is_enabled: bool,
    pub is_srv: bool,
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

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub enum RolloutPatchOp {
    /// Defines (or redefines) the schedule and immediately activates it, from step 0, in
    /// every environment of the project - not just the one the commit happens to be in.
    /// Progression itself still gates independently per environment (each has its own
    /// minimum-sample-size and hold-duration checks), so this only synchronizes the
    /// starting point, not the whole schedule.
    Set(RolloutConfig),
    /// Removes the rollout entirely - the schedule and every environment's progression,
    /// not just one.
    Unset,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate, ToSchema)]
pub struct FeaturePatch {
    #[validate(pattern = r"^[A-Za-z][A-Za-z0-9_]+$")]
    #[validate(max_length = 255)]
    pub name: Option<String>,
    pub is_enabled: Option<bool>,
    pub is_archived: Option<bool>,
    pub is_srv: Option<bool>,
    pub description: Option<String>,
    #[validate]
    pub tags: Vec<TagPatchOp>,
    pub variants: Vec<VariantPatchOp>,
    /// `None` means no change. Mirrors `is_enabled: Option<bool>`'s convention, just one
    /// level deeper since "change to what" itself needs a Set/Unset choice.
    pub rollout: Option<RolloutPatchOp>,
    /// Stages deletion of the entire feature. When set, every other field/op in the same
    /// patch is ignored - the feature is deleted and nothing else is applied.
    pub delete: bool,
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
            is_srv: feature.is_srv,
        }
    }
}

impl FeaturePatch {
    pub fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.is_enabled.is_none()
            && self.is_archived.is_none()
            && self.is_srv.is_none()
            && self.description.is_none()
            && self.tags.is_empty()
            && self.variants.is_empty()
            && self.rollout.is_none()
            && !self.delete
    }
}

/// A single staged variant override for one feature, carried inside [`IdentityPatch`].
/// The server resolves feature name to an existing feature and value to a variant.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct IdentityOverridePatch {
    pub feature_name: String,
    pub variant_value: VariantValue,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct IdentityPatch {
    pub traits: Vec<TraitPatchOp>,
    pub overrides: Vec<IdentityOverridePatch>,
    /// Feature names whose override should be unset (identity freed for distribution).
    pub unset_overrides: Vec<String>,
    /// Stages deletion of the entire identity. When set, every other field/op in the same
    /// patch is ignored - the identity is deleted and nothing else is applied.
    pub delete: bool,
}

impl IdentityPatch {
    pub fn is_empty(&self) -> bool {
        self.traits.is_empty()
            && self.overrides.is_empty()
            && self.unset_overrides.is_empty()
            && !self.delete
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct NewSegmentPayload {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SegmentVariantWeight {
    pub variant_id: i32,
    pub weight: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub enum SegmentPatchOp {
    SetName(String),
    SetDescription(Option<String>),
    AddGroup {
        connector: Option<GroupConnector>,
        description: Option<String>,
    },
    DeleteGroup {
        label: String,
    },
    SetGroupDescription {
        label: String,
        description: Option<String>,
    },
    AddRule {
        group_label: String,
        subject: Subject,
        comparator: Comparator,
        value: String,
    },
    DeleteRule {
        rule_id: i32,
    },
    SetRuleValue {
        rule_id: i32,
        value: String,
    },
    SetRuleComparator {
        rule_id: i32,
        comparator: Comparator,
    },
    /// Stores weight overrides for a feature's variants within this segment, scoped to
    /// whichever environment the enclosing commit targets (there's no per-op environment -
    /// a commit is already scoped to exactly one).
    SetFeatureOverride {
        feature_id: i32,
        variant_weights: Vec<SegmentVariantWeight>,
    },
    /// Removes all weight overrides for a feature within this segment, in the commit's
    /// environment.
    UnsetFeatureOverride {
        feature_id: i32,
    },
    /// Stages deletion of the entire segment. When present, every other op in the same
    /// patch is ignored - the segment is deleted and nothing else is applied.
    Delete,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct SegmentPatch {
    pub ops: Vec<SegmentPatchOp>,
}

impl SegmentPatch {
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}

/// One request's worth of a staged feature patch, addressed by id - part of
/// [`CommitPayload`].
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct FeatureCommitPart {
    pub id: i32,
    pub patch: FeaturePatch,
}

/// One request's worth of a staged identity patch, addressed by value - part of
/// [`CommitPayload`].
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct IdentityCommitPart {
    pub value: String,
    pub patch: IdentityPatch,
}

/// One request's worth of a staged segment patch, addressed by id - part of
/// [`CommitPayload`].
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SegmentCommitPart {
    pub id: i32,
    pub patch: SegmentPatch,
}

/// A single `COMMIT` as one atomic server-side operation: whichever of the feature,
/// identity, and segment contexts have pending changes are applied together in one
/// transaction, and every `(feature, environment)` pair affected by the combined change
/// gets exactly one new snapshot - never zero (silently missed), never more than one
/// (uncoordinated duplicate writes from what the user experienced as a single commit).
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CommitPayload {
    pub comment: Option<String>,
    pub feature: Option<FeatureCommitPart>,
    pub segment: Option<SegmentCommitPart>,
    pub identity: Option<IdentityCommitPart>,
}

impl CommitPayload {
    pub fn is_empty(&self) -> bool {
        self.feature.is_none() && self.identity.is_none() && self.segment.is_none()
    }
}

/// Result of a [`CommitPayload`]. Each of `feature`/`identity`/`segment` is only
/// meaningful when the corresponding request part was present - in that case `None`
/// means the patch staged a delete (mirrors the existing single-resource PATCH
/// endpoints' `Option<T>` convention), not "nothing happened."
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CommitResult {
    pub feature: Option<Feature>,
    pub segment: Option<Segment>,
    pub identity: Option<IdentityWithTraits>,
    pub snapshots: Vec<Snapshot>,
}

/// Request body for `SNAPSHOT restore` - an optional comment override for the new
/// snapshot the restore itself produces (defaults server-side to `restored from v{N}`).
#[derive(Debug, Default, Serialize, Deserialize, ToSchema)]
pub struct RestoreRequest {
    pub comment: Option<String>,
}

/// Request body for `SNAPSHOT describe` - replaces a snapshot's comment in place. `None`
/// clears it.
#[derive(Debug, Default, Serialize, Deserialize, ToSchema)]
pub struct UpdateSnapshotCommentPayload {
    pub comment: Option<String>,
}
