use std::collections::HashMap;
use std::fmt::Debug;
use std::ops::Deref;
use std::sync::Arc;

use digest_auth::{AuthContext, WwwAuthenticateHeader};
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Body, Method, RequestBuilder, Response};
use tokio::sync::Mutex;
use url::Url;

use crate::types::common::Dav2xx;
pub use crate::types::common::*;
use crate::types::list_cmd::{ListMultiStatus, ListResponse};
use crate::types::list_entities::{ListEntity, ListFile, ListFolder};

pub mod types;

pub mod re_exports;

#[cfg(test)]
mod tests;

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
        let mut builder = self.agent.request(method, url.as_str());
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
                    if response.status().as_u16() == 401 {
                        let headers = response.headers();
                        let www_auth = headers["www-authenticate"].to_str()?;
                        let digest_auth = digest_auth::parse(www_auth)?;
                        *lock = Some(digest_auth);
                        lock.clone().unwrap()
                    } else {
                        return Err(error(
                            Kind::Decode,
                            message("digest auth response code not 401"),
                        ));
                    }
                };
                let context = AuthContext::new(username, password, url.path());
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
        if reqwest_response.status().as_u16() == 207 {
            let response = reqwest_response.text().await?;
            let mul: ListMultiStatus = serde_xml_rs::from_str(&response)?;
            Ok(mul.responses)
        } else {
            Err(Error {
                inner: Box::new(Inner {
                    kind: Kind::Decode,
                    source: Some(Box::new(Message {
                        message: "list response code not 207".to_string(),
                    })),
                }),
            })
        }
    }

    /// List files and folders at the given path on Webdav server
    ///
    /// Depth of "0" applies only to the resource, "1" to the resource and it's children, "infinity" to the resource and all it's children recursively
    /// The result will contain an xml list with the remote folder contents.
    ///
    /// Use absolute path to the webdav server folder location
    pub async fn list(&self, path: &str, depth: Depth) -> Result<Vec<ListEntity>, Error> {
        let cmd_response = self.list_rsp(path, depth).await?;
        let mut entities: Vec<ListEntity> = vec![];
        for x in cmd_response {
            if x.prop_stat.prop.resource_type.redirect_ref.is_some()
                || x.prop_stat.prop.resource_type.redirect_lifetime.is_some()
            {
                return Err(Error {
                    inner: Box::new(Inner {
                        kind: Kind::Decode,
                        source: Some(Box::new(Message {
                            message: "redirect not support".to_string(),
                        })),
                    }),
                });
            }
            entities.push(if x.prop_stat.prop.resource_type.collection.is_some() {
                ListEntity::Folder(ListFolder {
                    href: x.href,
                    last_modified: x.prop_stat.prop.last_modified,
                    quota_used_bytes: x.prop_stat.prop.quota_used_bytes,
                    quota_available_bytes: x.prop_stat.prop.quota_available_bytes,
                    tag: x.prop_stat.prop.tag,
                })
            } else {
                ListEntity::File(ListFile {
                    href: x.href,
                    last_modified: x.prop_stat.prop.last_modified,
                    content_length: x
                        .prop_stat
                        .prop
                        .content_length
                        .ok_or(error(Kind::Decode, message("content_length not found")))?,
                    content_type: x
                        .prop_stat
                        .prop
                        .content_type
                        .ok_or(error(Kind::Decode, message("content_type not found")))?,
                    tag: x.prop_stat.prop.tag,
                })
            });
        }
        Ok(entities)
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
                .ok_or(error(Kind::Url, message("must set host")))?,
            auth: if let Some(auth) = self.auth {
                auth
            } else {
                Auth::Anonymous
            },
            digest_auth: Arc::new(Default::default()),
        })
    }
}
