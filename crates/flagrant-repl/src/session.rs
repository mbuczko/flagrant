use std::sync::RwLock;

#[derive(Debug)]
pub struct Session<T> {
    pub context: RwLock<T>,
}

impl<T> Session<T> {
    pub fn new(context: T) -> Self {
        Self {
            context: RwLock::new(context),
        }
    }
}
