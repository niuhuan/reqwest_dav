Reqwest dav
============

[![crates.io](https://img.shields.io/crates/v/reqwest_dav.svg)](https://crates.io/crates/reqwest_dav)
[![Documentation](https://docs.rs/reqwest_dav/badge.svg)](https://docs.rs/reqwest_dav)
[![MIT/Apache-2 licensed](https://img.shields.io/crates/l/reqwest.svg)](./LICENSE-APACHE)
[![CI](https://github.com/niuhuan/reqwest_dav/workflows/Rust/badge.svg)](https://github.com/niuhuan/reqwest_dav/actions?query=workflow%3ARust)


An async webdav client for rust with tokio and reqwest

## Features

- [x] Authentication
  - [x] Basic
  - [x] Digest
- [x] Files management
  - [x] Get
  - [x] Put
  - [x] Mv
  - [x] Cp
  - [x] Delete
  - [x] Mkcol
  - [x] List

## Examples

```rust
use crate::{Auth, ClientBuilder, Depth, Error};

#[tokio::test]
async fn it_works() -> Result<(), Error> {
  
    // build a client
    let client = ClientBuilder::new()
        .set_host("http://server/remote.php/dav/files/username/".to_string())
        .set_auth(Auth::Basic("username".to_owned(), "password".to_owned()))
        .build()?;

    // list files
    println!(
        "{}",
        serde_json::to_string(&client.list("", Depth::Infinity).await?).unwrap()
    );
  
    // delete a file
    client.delete("1.txt").await.unwrap();

    Ok(())
}
```
