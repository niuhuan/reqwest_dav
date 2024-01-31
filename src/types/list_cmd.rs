//! Types and serialisation expected for the PROPFIND command.

use crate::types::{DecodeError, Error, FieldError};
use chrono::{DateTime, Utc};
use serde_derive::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ListMultiStatus {
    #[serde(rename = "response")]
    pub responses: Vec<ListResponse>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListResponse {
    pub href: String,
    #[serde(rename = "propstat")]
    pub prop_stat: Vec<ListPropStat>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListPropStat {
    pub status: String,
    pub prop: ListProp,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ListResourceType {
    pub collection: Option<()>,
    #[serde(rename = "redirectref")]
    pub redirect_ref: Option<()>,
    // TODO: Pretty sure this is in the wrong place.
    #[serde(rename = "redirect-lifetime")]
    pub redirect_lifetime: Option<()>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListProp {
    #[serde(rename = "getlastmodified", deserialize_with = "http_time", default)]
    pub last_modified: Option<DateTime<Utc>>,
    #[serde(rename = "resourcetype", default)]
    pub resource_type: ListResourceType,
    #[serde(
        rename = "quota-used-bytes",
        deserialize_with = "empty_number",
        default
    )]
    pub quota_used_bytes: Option<i64>,
    #[serde(
        rename = "quota-available-bytes",
        deserialize_with = "empty_number",
        default
    )]
    pub quota_available_bytes: Option<i64>,
    #[serde(rename = "getetag")]
    pub tag: Option<String>,
    #[serde(
        rename = "getcontentlength",
        deserialize_with = "empty_number",
        default
    )]
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
    pub last_modified: DateTime<Utc>,
    pub content_length: i64,
    pub content_type: String,
    pub tag: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ListFolder {
    pub href: String,
    pub last_modified: DateTime<Utc>,
    pub quota_used_bytes: Option<i64>,
    pub quota_available_bytes: Option<i64>,
    pub tag: Option<String>,
}

fn status_is_ok(status: &str) -> bool {
    let code = status.split_whitespace().nth(1);

    match code {
        Some(code) => code.starts_with("2"),
        None => false,
    }
}

impl TryFrom<ListResponse> for ListEntity {
    type Error = crate::types::Error;
    fn try_from(response: ListResponse) -> Result<Self, Self::Error> {
        let valid_prop_stat = response
            .prop_stat
            .into_iter()
            .filter(|prop_stat| status_is_ok(&prop_stat.status))
            .next();

        match valid_prop_stat {
            Some(ListPropStat { prop, .. }) if prop.resource_type.collection.is_some() => {
                Ok(ListEntity::Folder(ListFolder {
                    href: response.href,
                    last_modified: prop.last_modified.ok_or_else(|| {
                        Error::Decode(DecodeError::FieldNotFound(FieldError {
                            field: "last_modified".to_owned(),
                        }))
                    })?,
                    quota_used_bytes: prop.quota_used_bytes,
                    quota_available_bytes: prop.quota_available_bytes,
                    tag: prop.tag,
                }))
            }
            Some(ListPropStat { prop, .. })
                if prop.resource_type.redirect_ref.is_some()
                    || prop.resource_type.redirect_lifetime.is_some() =>
            {
                Err(Error::Decode(DecodeError::FieldNotSupported(FieldError {
                    field: "redirect_ref".to_owned(),
                })))
            }
            Some(ListPropStat { prop, .. }) => Ok(ListEntity::File(ListFile {
                href: response.href,
                last_modified: prop.last_modified.ok_or_else(|| {
                    Error::Decode(DecodeError::FieldNotFound(FieldError {
                        field: "last_modified".to_owned(),
                    }))
                })?,
                content_length: prop.content_length.unwrap_or(0),
                content_type: prop.content_type.unwrap_or("".to_string()),
                tag: prop.tag,
            })),
            None => Err(Error::Decode(DecodeError::FieldNotFound(FieldError {
                field: "propstat with valid status".to_owned(),
            }))),
        }
    }
}

fn http_time<'de, D>(d: D) -> Result<Option<DateTime<Utc>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value: Option<String> = serde::Deserialize::deserialize(d)?;

    match value {
        None => Ok(None),
        Some(value) => match httpdate::parse_http_date(&value) {
            Ok(system_time) => Ok(Some(DateTime::<Utc>::from(system_time))),
            Err(_) => Err(serde::de::Error::custom("parse error")),
        },
    }
}

fn empty_number<'de, D>(d: D) -> Result<Option<i64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value: Option<String> = serde::Deserialize::deserialize(d)?;

    match value {
        None => Ok(None),
        Some(value) if value.is_empty() => Ok(None),
        Some(value) => match value.parse::<i64>() {
            Ok(number) => Ok(Some(number)),
            Err(_) => Err(serde::de::Error::custom("parse error")),
        },
    }
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
        let list_entity = ListEntity::try_from(response).unwrap();
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
        let list_entity = ListEntity::try_from(response).unwrap();
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

    /// Seen these cases in the wild so lets test for them.
    #[test]
    fn parse_multi_prop_stat_file() {
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
                        <D:getcontentlength>1234</D:getcontentlength>
                        <D:getcontenttype>application/text</D:getcontenttype>
                    </D:prop>
                </D:propstat>
                <D:propstat>
                    <D:status>HTTP/1.1 404 Not Found</D:status>
                    <D:prop>
                        <D:getcontenttype/>
                    </D:prop>
                </D:propstat>
            </D:response>
        </D:multistatus>"#;

        let parsed: ListMultiStatus = serde_xml_rs::from_str(xml).unwrap();
        assert_eq!(parsed.responses.len(), 1);
        let response = parsed.responses[0].clone();
        let list_entity = ListEntity::try_from(response).unwrap();
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

    #[test]
    fn parse_multi_prop_stat_folder() {
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
                <D:propstat>
                    <D:status>HTTP/1.1 404 Not Found</D:status>
                    <D:prop>
                    </D:prop>
                </D:propstat>
            </D:response>
        </D:multistatus>"#;

        let parsed: ListMultiStatus = serde_xml_rs::from_str(xml).unwrap();
        assert_eq!(parsed.responses.len(), 1);
        let response = parsed.responses[0].clone();
        let list_entity = ListEntity::try_from(response).unwrap();
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
    fn parse_multi_prop_stat_folder_with_empty_props() {
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
                <D:propstat>
                    <D:status>HTTP/1.1 404 Not Found</D:status>
                    <D:prop>
                        <D:quota-used-bytes/>
                    </D:prop>
                </D:propstat>
            </D:response>
        </D:multistatus>"#;

        let parsed: ListMultiStatus = serde_xml_rs::from_str(xml).unwrap();
        assert_eq!(parsed.responses.len(), 1);
        let response = parsed.responses[0].clone();
        let list_entity = ListEntity::try_from(response).unwrap();
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
    fn parse_redirect_ref_unsupported() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
        <D:multistatus xmlns:D="DAV:">
            <D:response>
                <D:href>/remote.php/dav/files/admin/file.txt</D:href>
                <D:propstat>
                    <D:status>HTTP/1.1 200 OK</D:status>
                    <D:prop>
                        <D:displayname>file.txt</D:displayname>
                        <D:getlastmodified>Wed, 10 Apr 2019 14:00:00 GMT</D:getlastmodified>
                        <D:resourcetype>
                            <D:redirectref/>
                        </D:resourcetype>
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
        let list_entity = ListEntity::try_from(response);
        assert!(list_entity.is_err());
    }

    /// Pretty sure this isn't where the redirect-lifetime response should be
    /// but testing this for consistency with the existing library.
    #[test]
    fn parse_redirect_lifetime_unsupported() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
        <D:multistatus xmlns:D="DAV:">
            <D:response>
                <D:href>/remote.php/dav/files/admin/file.txt</D:href>
                <D:propstat>
                    <D:status>HTTP/1.1 200 OK</D:status>
                    <D:prop>
                        <D:displayname>file.txt</D:displayname>
                        <D:getlastmodified>Wed, 10 Apr 2019 14:00:00 GMT</D:getlastmodified>
                        <D:resourcetype>
                            <D:redirect-lifetime/>
                        </D:resourcetype>
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
        let list_entity = ListEntity::try_from(response);
        assert!(list_entity.is_err());
    }
}
