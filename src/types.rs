use chrono::NaiveDateTime;
pub use common::*;
use serde::Serializer;

pub mod common {
    use std::fmt;
    use std::fmt::{Debug, Display, Formatter};

    use reqwest::Response;
    use serde_derive::{Deserialize, Serialize};

    pub enum Error {
        Reqwest(reqwest::Error),
        ReqwestDecode(ReqwestDecodeError),
        Decode(DecodeError),
    }

    pub enum DecodeError {
        DigestAuth(digest_auth::Error),
        SerdeXml(serde_xml_rs::Error),
        FieldNotSupported(FieldError),
        FieldNotFound(FieldError),
        StatusMismatched(StatusMismatchedError),
        Server(ServerError),
    }

    #[derive(Debug)]
    pub struct FieldError {
        pub field: String,
    }

    #[derive(Debug)]
    pub struct StatusMismatchedError {
        pub response_code: u16,
        pub expected_code: u16,
    }

    #[derive(Debug)]
    pub struct ServerError {
        pub response_code: u16,
        pub exception: String,
        pub message: String,
    }

    #[derive(Debug)]
    pub enum ReqwestDecodeError {
        Url(url::ParseError),
        HeaderToString(reqwest::header::ToStrError),
        InvalidHeaderValue(reqwest::header::InvalidHeaderValue),
        InvalidMethod(http::method::InvalidMethod),
    }

    impl Debug for Error {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            let mut builder = f.debug_struct("reqwest_dav::Error");
            match self {
                Error::Reqwest(err) => {
                    builder.field("kind", &"Reqwest");
                    builder.field("source", err);
                }
                Error::ReqwestDecode(err) => {
                    builder.field("kind", &"ReqwestDecode");
                    builder.field("source", err);
                }
                Error::Decode(err) => {
                    builder.field("kind", &"Decode");
                    builder.field("source", err);
                }
            }
            builder.finish()
        }
    }

    impl Display for Error {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            let mut builder = f.debug_struct("reqwest_dav::Error");
            match self {
                Error::Reqwest(err) => {
                    builder.field("kind", &"Reqwest");
                    builder.field("source", err);
                }
                Error::ReqwestDecode(err) => {
                    builder.field("kind", &"ReqwestDecode");
                    builder.field("source", err);
                }
                Error::Decode(err) => {
                    builder.field("kind", &"Decode");
                    builder.field("source", err);
                }
            }
            builder.finish()
        }
    }

    impl Debug for DecodeError {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            match self {
                Self::DigestAuth(arg0) => f.debug_tuple("DigestAuth").field(arg0).finish(),
                Self::SerdeXml(arg0) => f.debug_tuple("SerdeXml").field(arg0).finish(),
                Self::FieldNotSupported(arg0) => f.debug_tuple("NotSupported").field(arg0).finish(),
                Self::FieldNotFound(arg0) => f.debug_tuple("NotFound").field(arg0).finish(),
                Self::StatusMismatched(arg0) => {
                    f.debug_tuple("StatusMismatched").field(arg0).finish()
                }
                Self::Server(arg0) => f.debug_tuple("Server").field(arg0).finish(),
            }
        }
    }

    impl Display for DecodeError {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            match self {
                Self::DigestAuth(arg0) => f.debug_tuple("DigestAuth").field(arg0).finish(),
                Self::SerdeXml(arg0) => f.debug_tuple("SerdeXml").field(arg0).finish(),
                Self::FieldNotSupported(arg0) => f.debug_tuple("NotSupported").field(arg0).finish(),
                Self::FieldNotFound(arg0) => f.debug_tuple("NotFound").field(arg0).finish(),
                Self::StatusMismatched(arg0) => {
                    f.debug_tuple("StatusMismatched").field(arg0).finish()
                }
                Self::Server(arg0) => f.debug_tuple("Server").field(arg0).finish(),
            }
        }
    }

    impl std::error::Error for Error {}

    impl From<url::ParseError> for Error {
        fn from(error: url::ParseError) -> Self {
            Error::ReqwestDecode(ReqwestDecodeError::Url(error))
        }
    }

    impl From<reqwest::Error> for Error {
        fn from(error: reqwest::Error) -> Self {
            Error::Reqwest(error)
        }
    }

    impl From<reqwest::header::ToStrError> for Error {
        fn from(error: reqwest::header::ToStrError) -> Self {
            Error::ReqwestDecode(ReqwestDecodeError::HeaderToString(error))
        }
    }

    impl From<reqwest::header::InvalidHeaderValue> for Error {
        fn from(error: reqwest::header::InvalidHeaderValue) -> Self {
            Error::ReqwestDecode(ReqwestDecodeError::InvalidHeaderValue(error))
        }
    }

    impl From<http::method::InvalidMethod> for Error {
        fn from(error: http::method::InvalidMethod) -> Self {
            Error::ReqwestDecode(ReqwestDecodeError::InvalidMethod(error))
        }
    }

    impl From<digest_auth::Error> for Error {
        fn from(error: digest_auth::Error) -> Self {
            Error::Decode(DecodeError::DigestAuth(error))
        }
    }

    impl From<serde_xml_rs::Error> for Error {
        fn from(error: serde_xml_rs::Error) -> Self {
            Error::Decode(DecodeError::SerdeXml(error))
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct DavErrorTmp {
        pub exception: String,
        pub message: String,
    }

    #[async_trait::async_trait]
    pub trait Dav2xx {
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
                let tmp: DavErrorTmp = match serde_xml_rs::from_str(&text) {
                    Ok(tmp) => tmp,
                    Err(_) => {
                        return Err(Error::Decode(DecodeError::Server(ServerError {
                            response_code: code,
                            exception: "server exception and parse error".to_owned(),
                            message: text,
                        })))
                    }
                };
                Err(Error::Decode(DecodeError::Server(ServerError {
                    response_code: code,
                    exception: tmp.exception,
                    message: tmp.message,
                })))
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
