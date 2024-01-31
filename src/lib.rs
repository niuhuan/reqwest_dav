use std::collections::HashMap;
use std::fmt::Debug;
use std::ops::Deref;
use std::sync::Arc;

use digest_auth::{AuthContext, HttpMethod, WwwAuthenticateHeader};
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Body, Method, RequestBuilder, Response};
use tokio::sync::Mutex;
use url::Url;

use crate::types::list_cmd::{ListEntity, ListMultiStatus, ListResponse};
pub use crate::types::*;

pub mod types;

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
        match &self.auth {
            Auth::Anonymous => {}
            Auth::Basic(username, password) => {
                builder = builder.basic_auth(username, Some(password));
            }
            Auth::Digest(username, password) => {
                let mut lock = self.digest_auth.lock().await;
                let mut digest_auth = if let Some(digest_auth) = lock.deref() {
                    digest_auth.clone()
                } else {
                    let response = self.agent.get(url.as_str()).send().await?;
                    let code = response.status().as_u16();
                    if code == 401 {
                        let headers = response.headers();
                        let www_auth = headers
                            .get("www-authenticate")
                            .ok_or(Error::Decode(DecodeError::NoAuthHeaderInResponse))?
                            .to_str()?;
                        let digest_auth = digest_auth::parse(www_auth)?;
                        *lock = Some(digest_auth);
                        lock.clone().unwrap()
                    } else {
                        return Err(Error::Decode(DecodeError::StatusMismatched(
                            StatusMismatchedError {
                                response_code: code,
                                expected_code: 401,
                            },
                        )));
                    }
                };
                let mut context = AuthContext::new(username, password, url.path());
                context.method = HttpMethod::from(method.to_string());
                builder = builder.header(
                    "Authorization",
                    digest_auth.respond(&context)?.to_header_string(),
                );
            }
        };
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
        let code = reqwest_response.status().as_u16();
        if code == 207 {
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
                    response_code: code,
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

    pub fn build(self) -> Result<Client, Error> {
        Ok(Client {
            agent: if let Some(agent) = self.agent {
                agent
            } else {
                reqwest::Client::new()
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
