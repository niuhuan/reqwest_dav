use chrono::NaiveDateTime;
use serde::Serializer;

pub mod common {
    use std::fmt;
    use std::fmt::{Debug, Display, Formatter};

    use reqwest::Response;
    use serde_derive::{Deserialize, Serialize};

    pub(crate) type BoxError = Box<dyn std::error::Error + Send + Sync>;

    pub(crate) fn error<S: 'static + std::error::Error + Send + Sync>(
        kind: Kind,
        error: S,
    ) -> Error {
        Error {
            inner: Box::new(Inner {
                kind,
                source: Some(Box::new(error)),
            }),
        }
    }

    pub(crate) fn message<S: Into<String>>(msg: S) -> Message {
        Message {
            message: msg.into(),
        }
    }

    #[derive(Debug)]
    pub(crate) enum Kind {
        Reqwest,
        Decode,
        Url,
        Dav,
    }

    #[derive(Debug)]
    pub(crate) struct Inner {
        pub(crate) kind: Kind,
        pub(crate) source: Option<BoxError>,
    }

    pub struct Error {
        pub(crate) inner: Box<Inner>,
    }

    impl Debug for Error {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            let mut builder = f.debug_struct("reqwest_dav::Error");
            builder.field("kind", &self.inner.kind);
            if let Some(ref source) = self.inner.source {
                builder.field("source", source);
            }
            builder.finish()
        }
    }

    impl Display for Error {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            match self.inner.kind {
                Kind::Reqwest => f.write_str("reqwest error")?,
                Kind::Decode => f.write_str("decode error")?,
                Kind::Url => f.write_str("url error")?,
                Kind::Dav => f.write_str("dav error")?,
            };
            if let Some(e) = &self.inner.source {
                write!(f, ": {}", e)?;
            }
            Ok(())
        }
    }

    impl std::error::Error for Error {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            self.inner.source.as_ref().map(|e| &**e as _)
        }
    }

    impl From<url::ParseError> for Error {
        fn from(error: url::ParseError) -> Self {
            Error {
                inner: Box::new(Inner {
                    kind: Kind::Url,
                    source: Some(Box::new(error)),
                }),
            }
        }
    }

    impl From<reqwest::Error> for Error {
        fn from(error: reqwest::Error) -> Self {
            Error {
                inner: Box::new(Inner {
                    kind: Kind::Reqwest,
                    source: Some(Box::new(error)),
                }),
            }
        }
    }

    impl From<reqwest::header::ToStrError> for Error {
        fn from(error: reqwest::header::ToStrError) -> Self {
            Error {
                inner: Box::new(Inner {
                    kind: Kind::Reqwest,
                    source: Some(Box::new(error)),
                }),
            }
        }
    }

    impl From<reqwest::header::InvalidHeaderValue> for Error {
        fn from(error: reqwest::header::InvalidHeaderValue) -> Self {
            Error {
                inner: Box::new(Inner {
                    kind: Kind::Reqwest,
                    source: Some(Box::new(error)),
                }),
            }
        }
    }

    impl From<http::method::InvalidMethod> for Error {
        fn from(error: http::method::InvalidMethod) -> Self {
            Error {
                inner: Box::new(Inner {
                    kind: Kind::Reqwest,
                    source: Some(Box::new(error)),
                }),
            }
        }
    }

    impl From<digest_auth::Error> for Error {
        fn from(error: digest_auth::Error) -> Self {
            Error {
                inner: Box::new(Inner {
                    kind: Kind::Decode,
                    source: Some(Box::new(error)),
                }),
            }
        }
    }

    impl From<serde_xml_rs::Error> for Error {
        fn from(error: serde_xml_rs::Error) -> Self {
            Error {
                inner: Box::new(Inner {
                    kind: Kind::Decode,
                    source: Some(Box::new(error)),
                }),
            }
        }
    }

    #[derive(Clone)]
    pub struct Message {
        pub message: String,
    }

    impl Debug for Message {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            f.write_str(self.message.as_str())?;
            Ok(())
        }
    }

    impl Display for Message {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            f.write_str(self.message.as_str())?;
            Ok(())
        }
    }

    impl std::error::Error for Message {}

    impl From<Message> for Error {
        fn from(error: Message) -> Self {
            Error {
                inner: Box::new(Inner {
                    kind: Kind::Decode,
                    source: Some(Box::new(error)),
                }),
            }
        }
    }

    impl From<&str> for Message {
        fn from(str: &str) -> Self {
            Message {
                message: str.to_string(),
            }
        }
    }

    #[derive(Clone)]
    pub struct DavError {
        pub status_code: u16,
        pub exception: String,
        pub message: String,
    }

    impl Debug for DavError {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            f.write_str(&format!(
                "reqwest_dav::DavError {} {} , {} , {} {}",
                "{", self.status_code, self.exception, self.message, "}"
            ))?;
            Ok(())
        }
    }

    impl Display for DavError {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            f.write_str(&format!(
                "reqwest_dav::DavError {} {} , {} , {} {}",
                "{", self.status_code, self.exception, self.message, "}"
            ))?;
            Ok(())
        }
    }

    impl std::error::Error for DavError {}

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct DavErrorTmp {
        pub exception: String,
        pub message: String,
    }

    #[async_trait::async_trait]
    pub(crate) trait Dav2xx {
        async fn dav2xx(self) -> Result<Response, Error>;
    }

    #[async_trait::async_trait]
    impl Dav2xx for Response {
        async fn dav2xx(self) -> Result<Response, Error> {
            let code = self.status().as_u16();
            if code / 100 == 2 {
                Ok(self)
            } else {
                let text = self.text().await?;
                let tmp: DavErrorTmp = serde_xml_rs::from_str(&text)?;
                Err(error(
                    Kind::Dav,
                    DavError {
                        status_code: code,
                        exception: tmp.exception,
                        message: tmp.message,
                    },
                ))
            }
        }
    }

    #[derive(Debug, Clone)]
    pub enum Auth {
        Anonymous,
        Basic(String, String),
        Digest(String, String),
    }

    #[derive(Debug, Clone)]
    pub enum Depth {
        Number(i64),
        Infinity,
    }
}

pub mod list_cmd {
    use chrono::NaiveDateTime;
    use serde_derive::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ListMultiStatus {
        #[serde(rename = "response")]
        pub responses: Vec<ListResponse>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ListResponse {
        pub href: String,
        #[serde(rename = "propstat")]
        pub prop_stat: ListPropStat,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ListPropStat {
        pub status: String,
        pub prop: ListProp,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ListResourceType {
        pub collection: Option<()>,
        #[serde(rename = "redirectref")]
        pub redirect_ref: Option<()>,
        #[serde(rename = "redirect-lifetime")]
        pub redirect_lifetime: Option<()>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ListProp {
        #[serde(
            rename = "getlastmodified",
            deserialize_with = "super::fuzzy_time",
            serialize_with = "super::to_fuzzy_time"
        )]
        pub last_modified: NaiveDateTime,
        #[serde(rename = "resourcetype")]
        pub resource_type: ListResourceType,
        #[serde(rename = "quota-used-bytes")]
        pub quota_used_bytes: Option<i64>,
        #[serde(rename = "quota-available-bytes")]
        pub quota_available_bytes: Option<i64>,
        #[serde(rename = "getetag")]
        pub tag: Option<String>,
        #[serde(rename = "getcontentlength")]
        pub content_length: Option<i64>,
        #[serde(rename = "getcontenttype")]
        pub content_type: Option<String>,
    }
}

pub mod list_entities {
    use chrono::NaiveDateTime;
    use serde_derive::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum ListEntity {
        File(ListFile),
        Folder(ListFolder),
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ListFile {
        pub href: String,

        #[serde(
            rename = "getlastmodified",
            deserialize_with = "super::fuzzy_time",
            serialize_with = "super::to_fuzzy_time"
        )]
        pub last_modified: NaiveDateTime,

        #[serde(rename = "getcontentlength")]
        pub content_length: i64,

        #[serde(rename = "getcontenttype")]
        pub content_type: String,

        #[serde(rename = "getetag")]
        pub tag: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ListFolder {
        pub href: String,

        #[serde(
            rename = "getlastmodified",
            deserialize_with = "super::fuzzy_time",
            serialize_with = "super::to_fuzzy_time"
        )]
        pub last_modified: NaiveDateTime,

        #[serde(rename = "quota-used-bytes")]
        pub quota_used_bytes: Option<i64>,

        #[serde(rename = "quota-available-bytes")]
        pub quota_available_bytes: Option<i64>,

        #[serde(rename = "getetag")]
        pub tag: Option<String>,
    }
}

fn fuzzy_time<'de, D>(d: D) -> Result<NaiveDateTime, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value: String = serde::Deserialize::deserialize(d)?;
    match NaiveDateTime::parse_from_str(&value, "%a, %d %b %Y %H:%M:%S GMT") {
        Ok(from) => Ok(from),
        Err(_) => Err(serde::de::Error::custom("parse error")),
    }
}

fn to_fuzzy_time<S>(time: &NaiveDateTime, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(
        time.format("%a, %d %b %Y %H:%M:%S GMT")
            .to_string()
            .as_str(),
    )
}
