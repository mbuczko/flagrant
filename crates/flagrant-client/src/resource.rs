pub enum BaseResource<'a> {
    Project(i32),
    Environment(i32, &'a str),
}

impl BaseResource<'_> {
    pub fn subpath<S: AsRef<str>>(&self, subpath: S) -> String {
        let relative = subpath.as_ref();
        match self {
            BaseResource::Project(project_id) => format!("/projects/{project_id}{relative}"),
            BaseResource::Environment(project_id, env_name) => {
                format!("/projects/{project_id}/envs/{env_name}{relative}")
            }
        }
    }
}
