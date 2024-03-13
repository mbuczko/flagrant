use flagrant::models::{environment::Environment, project::Project};
use sqlx::{Pool, Sqlite};

#[derive(Debug)]
pub struct ReplContext {
    pub pool: Pool<Sqlite>,
    pub project: Project,
    pub env: Option<Environment>,
}

pub struct ReplContextBuilder {
    pub pool: Pool<Sqlite>,
    pub project: Project,
    pub env: Option<Environment>,
}


impl ReplContext {
    pub fn builder(project: Project, pool: Pool<Sqlite>) -> ReplContextBuilder {
        ReplContextBuilder { project, env: None, pool}
    }
    pub fn set_environment(&mut self, env: Environment) {
        self.env = Some(env);
    }
}

#[allow(dead_code)]
impl ReplContextBuilder {
    pub fn with_env(mut self, env: Environment) -> Self {
        self.env = Some(env);
        self
    }
    pub fn build(self) -> ReplContext {
        ReplContext {
            pool: self.pool,
            project: self.project,
            env: self.env
        }
    }
}
