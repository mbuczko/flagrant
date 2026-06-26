use anyhow::bail;
use flagrant_types::{
    Environment, Feature, FeatureResponse, IdentityWithTraits, Project, Segment,
    payload::{FeaturePatch, IdentityPatch, SegmentPatch},
};

use crate::{
    http::{Auth, HttpClient},
    resource::BaseResource,
};

/// A reference to a variant that is stable within a single listing session.
/// Committed variants are addressed by their DB id; staged additions are
/// addressed by their position (0-based) in the pending `Add` ops list.
#[derive(Debug, Clone)]
pub enum VariantRef {
    Committed(i32),
    Staged(usize),
}

#[derive(Debug)]
pub struct Connection {
    pub client: HttpClient,
    pub project: Project,
    pub environment: Environment,
    pub feature: Option<Feature>,
    pub feature_patch: Option<FeaturePatch>,
    /// Positional index that maps 1-based display index → VariantRef.
    /// Invalidated whenever pending ops change.
    pub variant_index: Vec<VariantRef>,
    /// Identity currently in context (set by `IDENTITY use`).
    pub identity: Option<IdentityWithTraits>,
    /// Staged patch for the current identity.
    pub identity_patch: Option<IdentityPatch>,
    /// Segment currently in context - mutually exclusive with identity context.
    pub segment: Option<Segment>,
    /// Staged patch for the current segment.
    pub segment_patch: Option<SegmentPatch>,
}

impl Connection {
    #[cfg(feature = "blocking")]
    pub fn init(
        api_host: String,
        auth: Auth,
        project_name: String,
        environment_id: i32,
    ) -> anyhow::Result<Connection> {
        let client = HttpClient::new(api_host, auth);
        let path = format!("/projects/{project_name}");

        Self::build(
            client.get::<Project>(path.clone()).ok(),
            client
                .get::<Environment>(format!("{path}/envs/{environment_id}"))
                .ok(),
            client,
        )
    }

    #[cfg(not(feature = "blocking"))]
    pub async fn init(
        api_host: String,
        project_name: String,
        environment_id: i32,
    ) -> anyhow::Result<Connection> {
        let client = HttpClient::new(api_host, Auth::None);
        let path = format!("/projects/{project_name}");

        Self::build(
            client.get::<Project>(path.clone()).await.ok(),
            client
                .get::<Environment>(format!("{path}/envs/{environment_id}"))
                .await
                .ok(),
            client,
        )
    }

    fn build(
        project: Option<Project>,
        environment: Option<Environment>,
        client: HttpClient,
    ) -> anyhow::Result<Connection> {
        match (project, environment) {
            (Some(project), Some(environment)) => Ok(Connection {
                client,
                project,
                environment,
                feature: None,
                feature_patch: None,
                variant_index: Vec::new(),
                identity: None,
                identity_patch: None,
                segment: None,
                segment_patch: None,
            }),
            (Some(_), None) => bail!("No environment of given id found."),
            (None, Some(_)) => bail!("No project of given id found."),
            _ => bail!("Neither project nor environment was found."),
        }
    }

    pub fn get_or_init_pending(&mut self) -> &mut FeaturePatch {
        self.feature_patch.get_or_insert_with(FeaturePatch::default)
    }

    pub fn has_feature_pending(&self) -> bool {
        self.feature_patch
            .as_ref()
            .map(|p| !p.is_empty())
            .unwrap_or(false)
    }

    pub fn discard_pending(&mut self) {
        self.feature_patch = None;
    }

    pub fn get_or_init_identity_patch(&mut self) -> &mut IdentityPatch {
        self.identity_patch
            .get_or_insert_with(IdentityPatch::default)
    }

    pub fn discard_identity_pending(&mut self) {
        self.identity_patch = None;
    }

    pub fn has_identity_pending(&self) -> bool {
        self.identity_patch
            .as_ref()
            .map(|p| !p.is_empty())
            .unwrap_or(false)
    }

    pub fn get_or_init_segment_patch(&mut self) -> &mut SegmentPatch {
        self.segment_patch.get_or_insert_with(SegmentPatch::default)
    }

    pub fn discard_segment_patch(&mut self) {
        self.segment_patch = None;
    }

    pub fn has_segment_pending(&self) -> bool {
        self.segment_patch
            .as_ref()
            .map(|p| !p.is_empty())
            .unwrap_or(false)
    }

    pub fn env_resource(&self) -> BaseResource<'_> {
        BaseResource::Environment(&self.project.name, &self.environment.name)
    }

    pub fn project_resource(&self) -> BaseResource<'_> {
        BaseResource::Project(&self.project.name)
    }

    #[cfg(feature = "blocking")]
    pub fn get_features(&self, identity: &str) -> Option<Vec<FeatureResponse>> {
        let path = self.env_resource().subpath("/features");
        self.client
            .get_with_identity(format!("/api/v1{path}"), Some(identity))
            .ok()
    }
}

pub trait Resource {
    fn as_base_resource(&self) -> BaseResource<'_>;
}

impl Resource for Project {
    fn as_base_resource(&self) -> BaseResource<'_> {
        BaseResource::Project(&self.name)
    }
}
