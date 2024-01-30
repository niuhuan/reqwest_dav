//! Types and serialisation expected for the PROPFIND command.

use chrono::NaiveDateTime;
use serde::Serializer;
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
        deserialize_with = "fuzzy_time",
        serialize_with = "to_fuzzy_time"
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

#[derive(Debug, Clone)]
pub enum ListEntity {
    File(ListFile),
    Folder(ListFolder),
}

#[derive(Debug, Clone)]
pub struct ListFile {
    pub href: String,
    pub last_modified: NaiveDateTime,
    pub content_length: i64,
    pub content_type: String,
    pub tag: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ListFolder {
    pub href: String,
    pub last_modified: NaiveDateTime,
    pub quota_used_bytes: Option<i64>,
    pub quota_available_bytes: Option<i64>,
    pub tag: Option<String>,
}

impl From<ListResponse> for ListEntity {
    fn from(response: ListResponse) -> Self {
        let prop = response.prop_stat.prop;
        match prop.resource_type.collection {
            Some(_) => ListEntity::Folder(ListFolder {
                href: response.href,
                last_modified: prop.last_modified,
                quota_used_bytes: prop.quota_used_bytes,
                quota_available_bytes: prop.quota_available_bytes,
                tag: prop.tag,
            }),
            None => ListEntity::File(ListFile {
                href: response.href,
                last_modified: prop.last_modified,
                content_length: prop.content_length.unwrap_or(0),
                content_type: prop.content_type.unwrap_or("".to_string()),
                tag: prop.tag,
            }),
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_prop_stat_folder() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
        <D:multistatus xmlns:D="DAV:">
            <D:response>
                <D:href>/remote.php/dav/files/admin</D:href>
                <D:propstat>
                    <D:status>HTTP/1.1 200 OK</D:status>
                    <D:prop>
                        <D:getlastmodified>Wed, 10 Apr 2019 14:00:00 GMT</D:getlastmodified>
                        <D:resourcetype>
                            <D:collection/>
                        </D:resourcetype>
                        <D:getetag>"5cafae80b1e3e"</D:getetag>
                        <D:getcontenttype>httpd/unix-directory</D:getcontenttype>
                    </D:prop>
                </D:propstat>
            </D:response>
        </D:multistatus>"#;

        let parsed: ListMultiStatus = serde_xml_rs::from_str(xml).unwrap();
        assert_eq!(parsed.responses.len(), 1);
        let response = parsed.responses[0].clone();
        let list_entity = ListEntity::from(response);
        match list_entity {
            ListEntity::Folder(folder) => {
                assert_eq!(folder.href, "/remote.php/dav/files/admin");
                assert_eq!(folder.last_modified.timestamp(), 1554904800);
                assert_eq!(folder.quota_used_bytes, None);
                assert_eq!(folder.quota_available_bytes, None);
                assert_eq!(folder.tag, Some("\"5cafae80b1e3e\"".to_string()));
            }
            _ => panic!("expected folder"),
        }
    }

    #[test]
    fn parse_single_prop_stat_file() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
        <D:multistatus xmlns:D="DAV:">
            <D:response>
                <D:href>/remote.php/dav/files/admin/file.txt</D:href>
                <D:propstat>
                    <D:status>HTTP/1.1 200 OK</D:status>
                    <D:prop>
                        <D:displayname>file.txt</D:displayname>
                        <D:getlastmodified>Wed, 10 Apr 2019 14:00:00 GMT</D:getlastmodified>
                        <D:resourcetype/>
                        <D:getetag>"5cafae80b1e3e"</D:getetag>
                        <D:getcontenttype>application/text</D:getcontenttype>
                        <D:getcontentlength>1234</D:getcontentlength>
                    </D:prop>
                </D:propstat>
            </D:response>
        </D:multistatus>"#;

        let parsed: ListMultiStatus = serde_xml_rs::from_str(xml).unwrap();
        assert_eq!(parsed.responses.len(), 1);
        let response = parsed.responses[0].clone();
        let list_entity = ListEntity::from(response);
        match list_entity {
            ListEntity::File(file) => {
                assert_eq!(file.href, "/remote.php/dav/files/admin/file.txt");
                assert_eq!(file.last_modified.timestamp(), 1554904800);
                assert_eq!(file.tag, Some("\"5cafae80b1e3e\"".to_string()));
                assert_eq!(file.content_length, 1234);
                assert_eq!(file.content_type, "application/text");
            }
            _ => panic!("expected folder"),
        }
    }
}
