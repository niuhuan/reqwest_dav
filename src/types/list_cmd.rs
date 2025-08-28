//! Types and serialisation expected for the PROPFIND command.

use crate::types::{DecodeError, Error, FieldError};
use chrono::{DateTime, Utc};
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
    pub prop_stat: Vec<ListPropStat>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListPropStat {
    pub status: String,
    pub prop: ListProp,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ListResourceType {
    pub collection: Option<()>,
    #[serde(rename = "redirectref")]
    pub redirect_ref: Option<()>,
    // TODO: Pretty sure this is in the wrong place.
    #[serde(rename = "redirect-lifetime")]
    pub redirect_lifetime: Option<()>,
    #[serde(rename = "addressbook", default)]
    pub address_book: Option<()>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    #[serde(rename = "calendar-data")]
    pub calendar_data: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ListEntity {
    File(ListFile),
    Folder(ListFolder),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListFile {
    pub href: String,
    pub last_modified: DateTime<Utc>,
    pub content_length: i64,
    pub content_type: String,
    pub tag: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListFolder {
    pub href: String,
    pub last_modified: DateTime<Utc>,
    pub quota_used_bytes: Option<i64>,
    pub quota_available_bytes: Option<i64>,
    pub tag: Option<String>,
    pub address_book: bool,
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
                    last_modified: if let Some(last_modified) = prop.last_modified {
                        last_modified
                    } else {
                        // When using Next Cloud's carddav, there maybe no `addressbook` flag, and no `getlastmodified` at all.
                        // return Err(Error::Decode(DecodeError::FieldNotFound(FieldError {
                        //     field: "last_modified".to_owned(),
                        // })));
                        let naive_date = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
                        let naive_date_date_time = naive_date.and_hms_opt(0, 0, 0).unwrap();
                        DateTime::<Utc>::from_naive_utc_and_offset(naive_date_date_time, Utc)
                    },
                    quota_used_bytes: prop.quota_used_bytes,
                    quota_available_bytes: prop.quota_available_bytes,
                    tag: prop.tag,
                    address_book: prop.resource_type.address_book.is_some(),
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
        Some(value) if value.is_empty() => Ok(None),
        Some(value) => match httpdate::parse_http_date(&value) {
            Ok(system_time) => Ok(Some(DateTime::<Utc>::from(system_time))),
            Err(_) => Err(serde::de::Error::invalid_value(
                serde::de::Unexpected::Str(&value),
                &"a valid HTTP date",
            )),
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
            Err(_) => Err(serde::de::Error::invalid_value(
                serde::de::Unexpected::Str(&value),
                &"a valid number",
            )),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::list_cmd::ListEntity::Folder;

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
    fn parse_single_prop_stat_with_empty_last_modified() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
        <D:multistatus xmlns:D="DAV:">
            <D:response>
                <D:href>/remote.php/dav/files/admin</D:href>
                <D:propstat>
                    <D:status>HTTP/1.1 200 OK</D:status>
                    <D:prop>
                        <D:getlastmodified></D:getlastmodified>
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
        assert_eq!(response.prop_stat.get(0).unwrap().prop.last_modified, None);
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

    /// test list for nextcloud carddav
    #[test]
    fn parse_carddav() {
        let xml = r#"<?xml version="1.0"?>
        <d:multistatus xmlns:d="DAV:" xmlns:s="http://sabredav.org/ns" xmlns:card="urn:ietf:params:xml:ns:carddav"
           xmlns:oc="http://owncloud.org/ns" xmlns:nc="http://nextcloud.org/ns">
                <d:response>
                    <d:href>/nextcloud/remote.php/dav/addressbooks/users/admin/</d:href>
                    <d:propstat>
                        <d:prop>
                            <d:resourcetype>
                                <d:collection/>
                            </d:resourcetype>
                        </d:prop>
                        <d:status>HTTP/1.1 200 OK</d:status>
                    </d:propstat>
                </d:response>
                <d:response>
                    <d:href>/nextcloud/remote.php/dav/addressbooks/users/admin/kontakte/</d:href>
                    <d:propstat>
                        <d:prop>
                            <d:resourcetype>
                                <d:collection/>
                                <card:addressbook/>
                            </d:resourcetype>
                        </d:prop>
                        <d:status>HTTP/1.1 200 OK</d:status>
                    </d:propstat>
                </d:response>
                <d:response>
                    <d:href>/nextcloud/remote.php/dav/addressbooks/users/admin/z-server-generated--system/Database:admin.vcf
                    </d:href>
                    <d:propstat>
                        <d:prop>
                            <d:getlastmodified>Sat, 12 Apr 2025 08:55:06 GMT</d:getlastmodified>
                            <d:getcontentlength>10525</d:getcontentlength>
                            <d:resourcetype/>
                            <d:getetag>&quot;9441c12dec6940919f049ef893dad0cd&quot;</d:getetag>
                            <d:getcontenttype>text/vcard; charset=utf-8</d:getcontenttype>
                        </d:prop>
                        <d:status>HTTP/1.1 200 OK</d:status>
                    </d:propstat>
                </d:response>
        </d:multistatus>"#;
        let parsed: ListMultiStatus = serde_xml_rs::from_str(xml).unwrap();
        assert_eq!(parsed.responses.len(), 3);
        let response = parsed.responses[1].clone();
        let list_entity = ListEntity::try_from(response).unwrap();
        if let Folder(folder) = list_entity {
            assert!(folder.address_book)
        } else {
            panic!("not folder")
        }
    }

    #[test]
    fn parse_calendar_data_prop() {
        let xml = r#"<?xml version="1.0"?>
        <d:multistatus xmlns:d="DAV:" xmlns:s="http://sabredav.org/ns" xmlns:cal="urn:ietf:params:xml:ns:caldav" 
            xmlns:cs="http://calendarserver.org/ns/" xmlns:oc="http://owncloud.org/ns" xmlns:nc="http://nextcloud.org/ns">
                <d:response>
                    <d:href>/remote.php/dav/calendars/user/personal/B3EECE08-5E62-407D-BD49-D8DCA03AC866.ics</d:href>
                    <d:propstat>
                        <d:prop>
                            <d:getetag>&quot;df71b3a3de483d6b5bccde1571d77639&quot;</d:getetag>
                            <d:getlastmodified>Tue, 12 Aug 2025 18:15:25 GMT</d:getlastmodified>
                            <d:getcontentlength>561</d:getcontentlength>
                            <d:getcontenttype>text/calendar; charset=utf-8; component=vevent</d:getcontenttype>
                            <cal:calendar-data>BEGIN:VCALENDAR
PRODID:-//IDN nextcloud.com//Calendar app 5.3.8//EN
CALSCALE:GREGORIAN
VERSION:2.0
BEGIN:VEVENT
CREATED:20250812T181515Z
DTSTAMP:20250812T181525Z
LAST-MODIFIED:20250812T181525Z
SEQUENCE:2
UID:29a07f82-706a-47eb-9d3b-3836d82851f6
DTSTART;TZID=Europe/Moscow:20250812T220015
DTEND;TZID=Europe/Moscow:20250812T230015
STATUS:CONFIRMED
SUMMARY:Test event
END:VEVENT
BEGIN:VTIMEZONE
TZID:Europe/Moscow
BEGIN:STANDARD
TZOFFSETFROM:+0300
TZOFFSETTO:+0300
TZNAME:MSK
DTSTART:19700101T000000
END:STANDARD
END:VTIMEZONE
END:VCALENDAR</cal:calendar-data>
                        </d:prop>
                        <d:status>HTTP/1.1 200 OK</d:status>
                    </d:propstat>
                </d:response>
        </d:multistatus>
        "#;

        let parsed: ListMultiStatus = serde_xml_rs::from_str(xml).unwrap();
        assert_eq!(parsed.responses.len(), 1);
        let response = parsed.responses[0].clone();
        let list_entity = ListEntity::try_from(response.clone()).unwrap();
        match list_entity {
            ListEntity::File(file) => {
                assert_eq!(file.href, "/remote.php/dav/calendars/user/personal/B3EECE08-5E62-407D-BD49-D8DCA03AC866.ics");
                assert_eq!(file.last_modified.timestamp(), 1755022525);
                assert_eq!(
                    file.tag,
                    Some("\"df71b3a3de483d6b5bccde1571d77639\"".to_string())
                );
                assert_eq!(file.content_length, 561);
                assert_eq!(
                    file.content_type,
                    "text/calendar; charset=utf-8; component=vevent"
                );
            }
            _ => panic!("expected folder"),
        }
        assert_eq!(response.prop_stat.len(), 1);

        let list_prop_stat = response.prop_stat[0].clone();
        assert_eq!(list_prop_stat.status, "HTTP/1.1 200 OK");

        assert!(list_prop_stat.prop.last_modified.is_some());
        assert_eq!(
            list_prop_stat.prop.last_modified.unwrap().timestamp(),
            1755022525
        );

        assert!(list_prop_stat.prop.calendar_data.is_some());
        assert_eq!(
            list_prop_stat.prop.calendar_data.unwrap(),
            r#"BEGIN:VCALENDAR
PRODID:-//IDN nextcloud.com//Calendar app 5.3.8//EN
CALSCALE:GREGORIAN
VERSION:2.0
BEGIN:VEVENT
CREATED:20250812T181515Z
DTSTAMP:20250812T181525Z
LAST-MODIFIED:20250812T181525Z
SEQUENCE:2
UID:29a07f82-706a-47eb-9d3b-3836d82851f6
DTSTART;TZID=Europe/Moscow:20250812T220015
DTEND;TZID=Europe/Moscow:20250812T230015
STATUS:CONFIRMED
SUMMARY:Test event
END:VEVENT
BEGIN:VTIMEZONE
TZID:Europe/Moscow
BEGIN:STANDARD
TZOFFSETFROM:+0300
TZOFFSETTO:+0300
TZNAME:MSK
DTSTART:19700101T000000
END:STANDARD
END:VTIMEZONE
END:VCALENDAR"#
        );
    }
}
