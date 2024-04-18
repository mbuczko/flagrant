pub enum BaseResource {
    Project(u16),
    Environment(u16),
}

impl BaseResource {
    pub fn subpath<S: AsRef<str>>(&self, subpath: S) -> String {
        let relative = subpath.as_ref();
        match self {
            BaseResource::Project(project_id) => format!("/projects/{project_id}{relative}"),
            BaseResource::Environment(environment_id) => format!("/envs/{environment_id}{relative}"),
        }
    }
}
