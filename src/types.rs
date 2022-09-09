use chrono::NaiveDateTime;
use serde::Serializer;

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
        pub tag: String,
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
        pub tag: String,
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
        pub quota_used_bytes: i64,

        #[serde(rename = "quota-available-bytes")]
        pub quota_available_bytes: i64,

        #[serde(rename = "getetag")]
        pub tag: String,
    }
}

fn fuzzy_time<'de, D>(d: D) -> std::result::Result<NaiveDateTime, D::Error>
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
