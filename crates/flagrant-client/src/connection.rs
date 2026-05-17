use anyhow::bail;
use flagrant_types::{
    Environment, Feature, FeatureResponse, IdentityWithTraits, Project, TraitValue,
    payload::FeaturePatch,
};
use std::collections::BTreeMap;

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
    /// Staged trait changes for the current identity.
    /// Value is `Some(encoded)` to upsert or `None` to remove.
    pub pending_traits: BTreeMap<String, Option<TraitValue>>,
}

impl Connection {
    #[cfg(feature = "blocking")]
    pub fn init(
        api_host: String,
        auth: Auth,
        project_id: i32,
        environment_id: i32,
    ) -> anyhow::Result<Connection> {
        let client = HttpClient::new(api_host, auth);
        let path = format!("/projects/{project_id}");

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
        project_id: i32,
        environment_id: i32,
    ) -> anyhow::Result<Connection> {
        let client = HttpClient::new(api_host, Auth::None);
        let path = format!("/projects/{project_id}");

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
                pending_traits: BTreeMap::new(),
            }),
            (Some(_), None) => bail!("No environment of given id found."),
            (None, Some(_)) => bail!("No project of given id found."),
            _ => bail!("Neither project nor environment was found."),
        }
    }

    pub fn get_or_init_pending(&mut self) -> &mut FeaturePatch {
        self.feature_patch.get_or_insert_with(FeaturePatch::default)
    }

    pub fn discard_pending(&mut self) {
        self.feature_patch = None;
    }

    pub fn discard_identity_pending(&mut self) {
        self.pending_traits.clear();
    }

    pub fn has_identity_pending(&self) -> bool {
        !self.pending_traits.is_empty()
    }

    #[cfg(feature = "blocking")]
    pub fn get_features(&self, identity: &str) -> Option<Vec<FeatureResponse>> {
        let path = self.environment.as_base_resource().subpath("/features");
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
        BaseResource::Project(self.id)
    }
}

impl Resource for Environment {
    fn as_base_resource(&self) -> BaseResource<'_> {
        BaseResource::Environment(self.project_id, &self.name)
    }
}
