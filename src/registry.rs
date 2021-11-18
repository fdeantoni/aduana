use std::collections::HashMap;

use serde::{Deserialize, Deserializer};

fn deserialize_null_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    T: Default + Deserialize<'de>,
    D: Deserializer<'de>,
{
    let opt = Option::deserialize(deserializer)?;
    Ok(opt.unwrap_or_default())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseCatalog {
    pub repositories: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseImage {
    pub name: String,
    pub tags: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseManifest {
    pub config: ResponseConfig,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseConfig {
    pub digest: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseConfigBlob {
    pub architecture: String,
    pub config: ConfigDetails,
    pub created: String,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct ConfigDetails {
    pub user: Option<String>,
    pub env: Vec<String>,
    pub cmd: Vec<String>,
    pub working_dir: Option<String>,
    #[serde(deserialize_with = "deserialize_null_default")]
    pub labels: HashMap<String, String>,
}