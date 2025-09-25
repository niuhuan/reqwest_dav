use std::collections::HashMap;
use std::fmt::Debug;
use std::fs::File;
use std::io::Read;
use std::sync::Arc;

use digest_auth::WwwAuthenticateHeader;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Body, Certificate, Method, RequestBuilder, Response};
use tokio::sync::Mutex;
use url::Url;

use crate::types::list_cmd::{ListEntity, ListMultiStatus, ListResponse};
pub use crate::types::*;

pub mod types;

mod authentication;
pub mod re_exports;

#[derive(Debug, Clone)]
pub struct Client {
    pub agent: reqwest::Client,
    pub host: String,
    pub auth: Auth,
    pub digest_auth: Arc<Mutex<Option<WwwAuthenticateHeader>>>,
}

#[derive(Debug, Clone)]
pub struct ClientBuilder {
    agent: Option<reqwest::Client>,
    host: Option<String>,
    auth: Option<Auth>,
}

impl Client {
    /// Main function that creates the RequestBuilder, sets the method, url and the basic_auth
    pub async fn start_request(&self, method: Method, path: &str) -> Result<RequestBuilder, Error> {
        let url = Url::parse(&format!(
            "{}/{}",
            self.host.trim_end_matches("/"),
            path.trim_start_matches("/")
        ))?;
        let mut builder = self.agent.request(method.clone(), url.as_str());
        builder = self.apply_authentication(builder, &method, &url).await?;
        Ok(builder)
    }

    pub async fn get_raw(&self, path: &str) -> Result<Response, Error> {
        Ok(self.start_request(Method::GET, path).await?.send().await?)
    }

    /// Get a file from Webdav server
    ///
    /// Use absolute path to the webdav server file location
    pub async fn get(&self, path: &str) -> Result<Response, Error> {
        self.get_raw(path).await?.dav2xx().await
    }

    pub async fn put_raw<B: Into<Body>>(&self, path: &str, body: B) -> Result<Response, Error> {
        Ok(self
            .start_request(Method::PUT, path)
            .await?
            .headers({
                let mut map = HeaderMap::new();
                map.insert(
                    "content-type",
                    HeaderValue::from_str("application/octet-stream")?,
                );
                map
            })
            .body(body)
            .send()
            .await?)
    }

    /// Upload a file/zip on Webdav server
    ///
    /// It can be any type of file as long as it is transformed to a vector of bytes (Vec<u8>).
    /// This can be achieved with **std::fs::File** or **zip-rs** for sending zip files.
    ///
    /// Use absolute path to the webdav server folder location
    pub async fn put<B: Into<Body>>(&self, path: &str, body: B) -> Result<(), Error> {
        self.put_raw(path, body).await?.dav2xx().await?;
        Ok(())
    }

    pub async fn delete_raw(&self, path: &str) -> Result<Response, Error> {
        Ok(self
            .start_request(Method::DELETE, path)
            .await?
            .send()
            .await?)
    }

    /// Deletes the collection, file, folder or zip archive at the given path on Webdav server
    ///
    /// Use absolute path to the webdav server file location
    pub async fn delete(&self, path: &str) -> Result<(), Error> {
        self.delete_raw(path).await?.dav2xx().await?;
        Ok(())
    }

    pub async fn mkcol_raw(&self, path: &str) -> Result<Response, Error> {
        Ok(self
            .start_request(Method::from_bytes(b"MKCOL").unwrap(), path)
            .await?
            .send()
            .await?)
    }

    /// Creates a directory on Webdav server
    ///
    /// Use absolute path to the webdav server file location
    pub async fn mkcol(&self, path: &str) -> Result<(), Error> {
        self.mkcol_raw(path).await?.dav2xx().await?;
        Ok(())
    }

    pub async fn unzip_raw(&self, path: &str) -> Result<Response, Error> {
        Ok(self
            .start_request(Method::POST, path)
            .await?
            .form(&{
                let mut params = HashMap::new();
                params.insert("method", "UNZIP");
                params
            })
            .send()
            .await?)
    }

    /// Unzips the .zip archieve on Webdav server
    ///
    /// Use absolute path to the webdav server file location
    pub async fn unzip(&self, path: &str) -> Result<(), Error> {
        self.unzip_raw(path).await?.dav2xx().await?;
        Ok(())
    }

    pub async fn mv_raw(&self, from: &str, to: &str) -> Result<Response, Error> {
        let base = Url::parse(&self.host)?;
        let mv_to = format!(
            "{}/{}",
            base.path().trim_end_matches("/"),
            to.trim_start_matches("/")
        );
        Ok(self
            .start_request(Method::from_bytes(b"MOVE")?, from)
            .await?
            .headers({
                let mut map = HeaderMap::new();
                map.insert("destination", HeaderValue::from_str(&mv_to)?);
                map
            })
            .send()
            .await?)
    }

    /// Rename or move a collection, file, folder on Webdav server
    ///
    /// If the file location changes it will move the file, if only the file name changes it will rename it.
    ///
    /// Use absolute path to the webdav server file location
    pub async fn mv(&self, from: &str, to: &str) -> Result<(), Error> {
        self.mv_raw(from, to).await?.dav2xx().await?;
        Ok(())
    }

    pub async fn cp_raw(&self, from: &str, to: &str, overwrite: bool) -> Result<Response, Error> {
        let base = Url::parse(&self.host)?;
        let cp_to = format!(
            "{}/{}",
            base.path().trim_end_matches("/"),
            to.trim_start_matches("/")
        );
        Ok(self
            .start_request(Method::from_bytes(b"COPY")?, from)
            .await?
            .headers({
                let mut map = HeaderMap::new();
                map.insert("destination", HeaderValue::from_str(&cp_to)?);
                map.insert(
                    "overwrite",
                    match overwrite {
                        true => HeaderValue::from_str("T")?,
                        false => HeaderValue::from_str("F")?,
                    },
                );
                map
            })
            .send()
            .await?)
    }

    /// Copy a collection, file, folder on Webdav server
    ///
    /// Use absolute path to the webdav server file location
    pub async fn cp(&self, from: &str, to: &str) -> Result<(), Error> {
        self.cp_raw(from, to, true).await?.dav2xx().await?;
        Ok(())
    }

    pub async fn list_raw(&self, path: &str, depth: Depth) -> Result<Response, Error> {
        let body = r#"<?xml version="1.0" encoding="utf-8" ?>
            <D:propfind xmlns:D="DAV:">
                <D:allprop/>
            </D:propfind>
        "#;
        Ok(self
            .start_request(Method::from_bytes(b"PROPFIND").unwrap(), path)
            .await?
            .headers({
                let mut map = HeaderMap::new();
                map.insert(
                    "depth",
                    HeaderValue::from_str(&match depth {
                        Depth::Number(value) => format!("{}", value),
                        Depth::Infinity => "infinity".to_owned(),
                    })?,
                );
                map
            })
            .body(body)
            .send()
            .await?)
    }

    pub async fn list_rsp(&self, path: &str, depth: Depth) -> Result<Vec<ListResponse>, Error> {
        let reqwest_response = self.list_raw(path, depth).await?;
        let code = reqwest_response.status();
        if code.is_success() {
            let response = reqwest_response.text().await?;
            let result: Result<ListMultiStatus, serde_xml_rs::Error> =
                serde_xml_rs::from_str(&response);
            match result {
                Ok(mul) => Ok(mul.responses),
                Err(e) => {
                    println!("Error: {}", e);
                    Err(e.into())
                }
            }
        } else {
            Err(Error::Decode(DecodeError::StatusMismatched(
                StatusMismatchedError {
                    response_code: code.as_u16(),
                    expected_code: 207,
                },
            )))
        }
    }

    /// List files and folders at the given path on Webdav server
    ///
    /// Depth of "0" applies only to the resource, "1" to the resource and it's children, "infinity" to the resource and all it's children recursively
    /// The result will contain an xml list with the remote folder contents.
    ///
    /// Use absolute path to the webdav server folder location
    pub async fn list(&self, path: &str, depth: Depth) -> Result<Vec<ListEntity>, Error> {
        let responses = self.list_rsp(path, depth).await?;
        responses.into_iter().map(ListEntity::try_from).collect()
    }
}

impl ClientBuilder {
    pub fn new() -> Self {
        Self {
            agent: None,
            host: None,
            auth: None,
        }
    }

    pub fn set_agent(mut self, agent: reqwest::Client) -> Self {
        self.agent = Some(agent);
        self
    }

    pub fn set_host(mut self, host: String) -> Self {
        self.host = Some(host);
        self
    }

    pub fn set_auth(mut self, auth: Auth) -> Self {
        self.auth = Some(auth);
        self
    }

    fn is_pem_format(&self, path: &str) -> bool {
        let mut result = false;
        if let Ok(mut file) = File::open(path) {
            let mut buffer = [0u8; 30];
            if let Ok(_) = file.read_exact(&mut buffer) {
                result = std::str::from_utf8(&buffer)
                    .map(|s| s.to_uppercase().contains("-----BEGIN"))
                    .unwrap_or(false);
            }
        }
        result
    }

    pub fn build(self, ignore_cert: bool, server_cert: Option<String>) -> Result<Client, Error> {
        Ok(Client {
            agent: if let Some(agent) = self.agent {
                agent
            } else {
                let mut builder =
                    reqwest::Client::builder().danger_accept_invalid_certs(ignore_cert);
                if let Some(path) = server_cert {
                    if let Ok(mut file) = File::open(&path) {
                        let mut buf = Vec::new();
                        if let Ok(_) = file.read_to_end(&mut buf) {
                            if let Ok(cert) = match self.is_pem_format(&path) {
                                true => Certificate::from_pem(&buf),
                                false => Certificate::from_der(&buf),
                            } {
                                builder = builder.add_root_certificate(cert);
                            }
                        }
                    }
                }
                builder.build()?
            },
            host: self
                .host
                .ok_or(Error::Decode(DecodeError::FieldNotFound(FieldError {
                    field: "host".to_owned(),
                })))?,
            auth: if let Some(auth) = self.auth {
                auth
            } else {
                Auth::Anonymous
            },
            digest_auth: Arc::new(Default::default()),
        })
    }
}
