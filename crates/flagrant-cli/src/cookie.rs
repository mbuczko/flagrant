use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Cookie {
    pub features: Vec<Feature>,
    pub traits: Vec<String>,
    pub ident: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Feature {
    pub feature_id: String,
    pub variant_id: Option<String>,
    pub version: u16,
}
