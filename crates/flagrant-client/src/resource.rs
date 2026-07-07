pub enum BaseResource<'a> {
    Project(&'a str),
    Environment(&'a str, &'a str),
}

impl BaseResource<'_> {
    pub fn subpath<S: AsRef<str>>(&self, subpath: S) -> String {
        let relative = subpath.as_ref();
        match self {
            BaseResource::Project(project) => format!("/projects/{project}{relative}"),
            BaseResource::Environment(project, env_name) => {
                format!("/projects/{project}/envs/{env_name}{relative}")
            }
        }
    }
}
