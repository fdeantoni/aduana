//! A Simple Crate to Extract Image Details from a Docker Registry
//!
//! This crate provides a simple interface to retrieve all the images
//! stored on a private registry, and retrieve the details per image as
//! needed.
//!
//! Example:
//! ```
//! use aduana::*;
//!
//! #[tokio::main]
//! pub async fn main() -> Result<(), AduanaError> {
//!
//!     // Create an inspector instance pointing to your registry
//!     let inspector = AduanaInspector::new("http://localhost:5000");
//!     // Retrieve a list of images on the registry
//!     let images = inspector.images().await?;
//!
//!     // Loop over the retrieved images
//!     for image in images {
//!         // For each tag of an image
//!         for tag in image.tags() {
//!             // Retrieve its details
//!             let details = image.details(tag).await?;
//!             println!("{:#?}", details);
//!         }
//!     }
//! }
//! ```

mod registry;

use std::collections::HashMap;

use anyhow::{anyhow, Context, Result};
use reqwest::{Certificate, Client, header::ACCEPT};
use thiserror::Error;

use registry::*;

#[derive(Error, Debug)]
pub enum AduanaError {
    #[error("Cannot connect to {url}: {reason}")]
    Connection { url: String, reason: String },
    #[error(transparent)]
    Runtime(#[from] anyhow::Error),
}

impl From<reqwest::Error> for AduanaError {
    fn from(error: reqwest::Error) -> Self {
        log::error!("Reqwest error: {:?}", &error);
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

#[derive(Debug, Clone)]
pub struct AduanaImage<'a> {
    inspector: &'a AduanaInspector,
    name: String,
    tags: Vec<String>,
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

fn client(pem: &Option<Vec<u8>>) -> Result<Client, AduanaError> {
    let mut builder = reqwest::Client::builder();

    if let Some(bytes) = pem {
        let cert = Certificate::from_pem(bytes).with_context(||"Failed to parse PEM certificate".to_string())?;
        builder = builder.add_root_certificate(cert);
    }

    let client = builder.build().with_context(||"Failed to build client!")?;
    println!("Client: {:#?}", &client);

    Ok(client)
}

impl<'a> AduanaImage<'a> {
    /// The name of an image
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The tags of this image
    pub fn tags(&self) -> &[String] {
        &self.tags
    }

    /// Retrieve the image details for a specific tag.
    pub async fn details(&self, tag: &str) -> Result<ImageDetails, AduanaError> {
        let url = format!(
            "{}/v2/{}/manifests/{}",
            &self.inspector.url, &self.name, tag
        );
        let client = client(&self.inspector.cert)?;
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
        let client = client(&self.inspector.cert)?;
        let response = client.get(&url).send().await?;
        let details: ResponseConfigBlob = response.json().await?;
        Ok(details)
    }
}

#[derive(Clone)]
pub struct AduanaInspector {
    url: String,
    cert: Option<Vec<u8>>,
}

impl std::fmt::Debug for AduanaInspector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AduanaInspector {{ url: {}, cert: {} }}", &self.url, self.cert.is_some())
    }
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
        let client = client(&self.cert)?;
        let response = client.get(&url).send().await?;

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
        let client = client(&self.cert)?;
        let response = client.get(&url).send().await?;
        let image: ResponseImage = response.json().await?;
        Ok(image)
    }
}

#[cfg(test)]
mod tests {

    use std::fs::File;
    use std::io::Read;

    use super::*;

    fn init() {
        if std::env::var("RUST_LOG").is_err() {
            std::env::set_var("RUST_LOG", "info,aduana=trace");
        }
        let _ = env_logger::builder().is_test(true).try_init();
    }

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
    async fn test_cert() {
        init();

        let mut pem = Vec::new();
        let mut file = File::open("certs/registry.crt").unwrap();
        file.read_to_end(&mut pem).unwrap();

        let inspector = AduanaInspector::new("https://localhost:5000").with_cert(pem);
        let images = inspector.images().await.unwrap();
        println!("{:?}", images);
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
