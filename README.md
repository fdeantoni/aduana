# Aduana
[![crates.io](https://buildstats.info/crate/aduana)](https://crates.io/crates/aduana) [![build](https://github.com/fdeantoni/aduana/actions/workflows/rust.yml/badge.svg)](https://github.com/fdeantoni/aduana/actions/workflows/rust.yml)

A very simple reqwest based crate to gather image info from a private docker
registry.

This crate provides a simple interface to retrieve all the images stored on a
private registry, and retrieve the details per image as needed. To use it,
add the following to your `Cargo.toml`:
```toml
[dependencies]
aduana = "0.1"
```

An example:
```rust
use aduana::*;

#[tokio::main]
pub async fn main() -> Result<(), AduanaError> {

    // Create an inspector instance pointing to your registry
    let inspector = AduanaInspector::new("http://localhost:5000");
    // Retrieve a list of images on the registry
    let images = inspector.images().await?;

    // Loop over the retrieved images
    for image in images {
        // For each tag of an image
        for tag in image.tags() {
            // Retrieve its details
            let details = image.details(tag).await?;
            println!("{:#?}", details);
        }
    }

    Ok(())
}
```


## Local Registry For Development and Testing

Create a certificate for development. I would recommend using [mkcert](https://github.com/FiloSottile/mkcert).

To test things out, you can run a local docker registry as follows:
```sh
$ docker run -it --rm \
    -p 5000:5000 \
    -v "$(pwd)"/certs:/certs \
    -e REGISTRY_HTTP_ADDR=0.0.0.0:5000 \
    -e REGISTRY_HTTP_TLS_CERTIFICATE=/certs/registry.crt \
    -e REGISTRY_HTTP_TLS_KEY=/certs/registry.key \
    registry:2
```

In a separate console, pull, retag, and push an image to your local test
registry as follows:
```sh
$ docker pull alpine
$ docker tag alpine localhost:5000/alpine:latest
$ docker push localhost:5000/alpine:latest
```

For more info, see the [docker docs](https://docs.docker.com/registry/insecure/).

## Todos

For now this crate is only meant for use with a small local registry containing
a limited set of images. It does not implement any filtering or pagination to
collect all the tags each image has. Do not use it on very large repository
you do not own as you will clobber it!
