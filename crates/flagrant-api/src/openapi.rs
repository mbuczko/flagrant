use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        crate::handlers::projects::list,
        crate::handlers::projects::fetch,
        crate::handlers::projects::create,
        crate::handlers::environments::list,
        crate::handlers::environments::fetch_by_id_or_name,
        crate::handlers::environments::create,
        crate::handlers::features::list,
        crate::handlers::features::fetch_by_id_or_name,
        crate::handlers::features::create,
        crate::handlers::features::update,
        crate::handlers::features::delete,
        crate::handlers::features::patch,
        crate::handlers::variants::list,
        crate::handlers::variants::fetch,
        crate::handlers::variants::create,
        crate::handlers::variants::update,
        crate::handlers::variants::delete,
        crate::handlers::tags::list,
        crate::api::get_features,
    ),
    components(
        schemas(
            flagrant_types::Project,
            flagrant_types::Environment,
            flagrant_types::Feature,
            flagrant_types::Variant,
            flagrant_types::FeatureValue,
            flagrant_types::Tag,
            flagrant_types::TagList,
            flagrant_types::FeatureResponse,
            flagrant_types::payload::ProjectRequestPayload,
            flagrant_types::payload::EnvRequestPayload,
            flagrant_types::payload::FeatureRequestPayload,
            flagrant_types::payload::VariantRequestPayload,
            flagrant_types::payload::FeaturePatch,
            flagrant_types::payload::VariantPatchOp,
        )
    ),
    tags(
        (name = "projects", description = "Project management"),
        (name = "environments", description = "Environment management"),
        (name = "features", description = "Feature flag management"),
        (name = "variants", description = "Feature variant management"),
        (name = "tags", description = "Tag management"),
        (name = "api", description = "Public client API"),
    ),
    info(
        title = "Flagrant API",
        version = "0.0.3",
        description = "CLI-powered feature-flagging service"
    )
)]
pub struct ApiDoc;
