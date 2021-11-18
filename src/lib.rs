use std::collections::HashMap;

use anyhow::{anyhow, Context, Result};
use reqwest::header::ACCEPT;
use serde::{Deserialize, Deserializer};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AduanaError {
    #[error("Cannot connect to {url}: {reason}")]
    Connection { url: String, reason: String },
    #[error(transparent)]
    Runtime(#[from] anyhow::Error),
}

impl From<reqwest::Error> for AduanaError {
    fn from(error: reqwest::Error) -> Self {
        if error.is_connect() || error.is_builder() {
            let url = error
                .url()
                .map(|url| url.to_string())
                .unwrap_or_else(|| "invalid".to_string());
            AduanaError::Connection {
                url,
                reason: error.to_string(),
            }
        } else {
            AduanaError::Runtime(anyhow!(
                "Failed to get images from {:?}. {:?}",
                error.url(),
                error
            ))
        }
    }
}

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
struct ResponseCatalog {
    repositories: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseImage {
    name: String,
    tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AduanaImage<'a> {
    inspector: &'a AduanaInspector,
    name: String,
    tags: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResponseManifest {
    config: ResponseConfig,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResponseConfig {
    digest: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResponseConfigBlob {
    architecture: String,
    config: ConfigDetails,
    created: String,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
struct ConfigDetails {
    pub user: Option<String>,
    pub env: Vec<String>,
    pub cmd: Vec<String>,
    pub working_dir: Option<String>,
    #[serde(deserialize_with = "deserialize_null_default")]
    pub labels: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct ImageDetails {
    pub name: String,
    pub tag: String,
    pub user: Option<String>,
    pub env: Vec<String>,
    pub cmd: Vec<String>,
    pub working_dir: Option<String>,
    pub labels: HashMap<String, String>,
    pub arch: String,
    pub created: String,
}

impl<'a> AduanaImage<'a> {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn tags(&self) -> &[String] {
        &self.tags
    }

    pub async fn details(&self, tag: &str) -> Result<ImageDetails, AduanaError> {
        let url = format!(
            "{}/v2/{}/manifests/{}",
            &self.inspector.url, &self.name, tag
        );
        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .header(
                ACCEPT,
                "application/vnd.docker.distribution.manifest.v2+json",
            )
            .send()
            .await?;
        let manifest: ResponseManifest = response.json().await?;
        let blob = self.retrieve_blob(&manifest.config.digest).await?;

        let result = ImageDetails {
            name: self.name.clone(),
            tag: tag.to_string(),
            user: blob.config.user,
            env: blob.config.env,
            cmd: blob.config.cmd,
            working_dir: blob.config.working_dir,
            labels: blob.config.labels,
            arch: blob.architecture,
            created: blob.created,
        };

        Ok(result)
    }

    async fn retrieve_blob(&self, digest: &str) -> Result<ResponseConfigBlob, AduanaError> {
        let url = format!("{}/v2/{}/blobs/{}", &self.inspector.url, &self.name, digest);
        let client = reqwest::Client::new();
        let response = client.get(&url).send().await?;
        let details: ResponseConfigBlob = response.json().await?;
        Ok(details)
    }
}

#[derive(Debug, Clone)]
pub struct AduanaInspector {
    url: String,
    cert: Option<Vec<u8>>,
}

impl AduanaInspector {
    pub fn new(url: &str) -> Self {
        AduanaInspector {
            url: url.to_string(),
            cert: None,
        }
    }

    pub fn with_cert(mut self, pem: Vec<u8>) -> Self {
        self.cert = Some(pem);
        self
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub async fn images(&'_ self) -> Result<Vec<AduanaImage<'_>>, AduanaError> {
        let url = format!("{}/v2/_catalog", &self.url);
        let response = reqwest::get(&url).await?;

        let mut images = Vec::new();
        let catalog: ResponseCatalog = response
            .json()
            .await
            .with_context(|| "Failed to parse catalog response")?;
        for name in catalog.repositories {
            let image = self.retrieve_image(&name).await?;
            let image = AduanaImage {
                inspector: self,
                name: image.name,
                tags: image.tags,
            };
            images.push(image);
        }
        Ok(images)
    }

    async fn retrieve_image(&self, name: &str) -> Result<ResponseImage, AduanaError> {
        let url = format!("{}/v2/{}/tags/list", &self.url, name);
        let response = reqwest::get(&url).await?;
        let image: ResponseImage = response.json().await?;
        Ok(image)
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test]
    async fn test_images() {
        let inspector = AduanaInspector::new("http://localhost:5000");
        let images = inspector.images().await.unwrap();
        println!("{:#?}", images);
    }

    #[tokio::test]
    async fn test_details() {
        let inspector = AduanaInspector::new("http://localhost:5000");
        let images = inspector.images().await.unwrap();

        for image in images {
            for tag in image.tags() {
                let details = image.details(tag).await.unwrap();
                println!("{:#?}", details);
            }
        }
    }

    #[tokio::test]
    async fn wrong_url() {
        let inspector = AduanaInspector::new(":xx:x");
        match inspector.images().await {
            Err(AduanaError::Connection { url, reason: _ }) => {
                assert_eq!(&url, "invalid");
            }
            Err(other) => panic!("Unexpected error! {:#?}", other),
            Ok(result) => panic!("Should not get result back! {:#?}", result),
        }
    }
}
