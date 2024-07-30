//! Implements the authentication logic for the server.

use crate::types::Error;
use crate::{Auth, Client, DecodeError, StatusMismatchedError};
use digest_auth::{AuthContext, HttpMethod};
use http::Method;
use reqwest::RequestBuilder;
use std::ops::Deref;
use url::Url;

impl Client {
    /// Apply the current auth method to the request.
    pub(crate) async fn apply_authentication(
        &self,
        mut builder: RequestBuilder,
        method: &Method,
        url: &Url,
    ) -> Result<RequestBuilder, Error> {
        match &self.auth {
            Auth::Anonymous => {}
            Auth::Basic(username, password) => {
                builder = builder.basic_auth(username, Some(password));
            }
            Auth::Digest(username, password) => {
                self.setup_digest_auth_if_not_initialized(method, url)
                    .await?;
                let mut context = AuthContext::new(username, password, url.path());
                context.method = HttpMethod::from(method.to_string());
                let mut digest_state_lock = self.digest_auth.lock().await;
                match digest_state_lock.as_mut() {
                    // This should be unreachable unless a silent error occurs in the setup_digest_auth_if_not_initialized function.
                    None => return Err(Error::MissingAuthContext),
                    Some(state) => {
                        let response = state.respond(&context)?;
                        builder = builder.header("Authorization", response.to_header_string());
                    }
                }
            }
        };
        Ok(builder)
    }

    /// Get the setup status of the digest auth context.
    ///
    /// Self contained in a function to make the lock bounds limited and clear.
    async fn is_digest_auth_initialized(&self) -> bool {
        self.digest_auth.lock().await.deref().is_some()
    }

    /// Setup the digest auth context if it is not already setup.
    async fn setup_digest_auth_if_not_initialized(
        &self,
        method: &Method,
        url: &Url,
    ) -> Result<(), Error> {
        if !self.is_digest_auth_initialized().await {
            self.probe_server_for_digest_auth(method, url).await?;
        }
        Ok(())
    }

    /// Make a request with the intention of getting a 401 error and updating the authorisation.
    async fn probe_server_for_digest_auth(&self, method: &Method, url: &Url) -> Result<(), Error> {
        let response = self
            .agent
            .request(method.clone(), url.as_str())
            .send()
            .await?;
        let code = response.status().as_u16();
        if code == 401 {
            let headers = response.headers();
            let www_auth = headers
                .get("www-authenticate")
                .ok_or(Error::Decode(DecodeError::NoAuthHeaderInResponse))?
                .to_str()?;
            self.update_auth_context(www_auth).await?;
            Ok(())
        } else {
            Err(Error::Decode(DecodeError::StatusMismatched(
                StatusMismatchedError {
                    response_code: code,
                    expected_code: 401,
                },
            )))
        }
    }

    /// Update the authentication context which right now is just
    /// for digest authentication.
    async fn update_auth_context(&self, auth_header: &str) -> Result<(), Error> {
        let auth_context = digest_auth::parse(auth_header)?;
        let mut session_auth = self.digest_auth.lock().await;
        *session_auth = Some(auth_context);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{Auth, Client, ClientBuilder, Depth};
    use std::time::Duration;
    use wiremock::matchers::{basic_auth, header_exists, method};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn setup_digest_client(host: String) -> Client {
        let reqwest_client = reqwest::ClientBuilder::new()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap();
        ClientBuilder::new()
            .set_host(host)
            .set_auth(Auth::Digest("user".to_owned(), "password".to_owned()))
            .set_agent(reqwest_client)
            .build()
            .unwrap()
    }

    #[tokio::test]
    async fn can_update_auth_context_with_valid_header() {
        let client = setup_digest_client("http://example.com".to_owned());
        let auth_header = "Digest realm=\"example.com\", qop=\"auth\", nonce=\"dcd98b7102dd2f0e8b11d0f600bfb0c093\", opaque=\"5ccc069c403ebaf9f0171e9517f40e41\"";
        client.update_auth_context(auth_header).await.unwrap();
        let auth_context = client.digest_auth.lock().await;
        assert!(auth_context.is_some());

        // not concerned with all fields as that is handled by the digest crate. Just make sure something matches.
        assert_eq!(auth_context.as_ref().unwrap().realm, "example.com");
    }

    #[tokio::test]
    async fn can_updated_existing_auth_context() {
        let client = setup_digest_client("http://example.com".to_owned());
        let auth_header = "Digest realm=\"example.com\", qop=\"auth\", nonce=\"dcd98b7102dd2f0e8b11d0f600bfb0c093\", opaque=\"5ccc069c403ebaf9f0171e9517f40e41\"";
        client.update_auth_context(auth_header).await.unwrap();
        let auth_header_2 = "Digest realm=\"example.com\", qop=\"auth\", nonce=\"notthesame\", opaque=\"5ccc069c403ebaf9f0171e9517f40e41\"";
        client.update_auth_context(auth_header_2).await.unwrap();
        let auth_context = client.digest_auth.lock().await;
        assert!(auth_context.is_some());

        // not concerned with all fields as that is handled by the digest crate. Just make sure something matches.
        assert_eq!(auth_context.as_ref().unwrap().nonce, "notthesame");
    }

    #[tokio::test]
    async fn returns_error_on_bad_header() {
        let client = setup_digest_client("http://example.com".to_owned());
        let auth_header = "Digest realm=\"example.com\", qop=\"auth\", opaque=\"5ccc069c403ebaf9f0171e9517f40e41\"";
        let result = client.update_auth_context(auth_header).await;
        assert!(result.is_err());
        let auth_context = client.digest_auth.lock().await;
        assert!(auth_context.is_none());
    }

    #[tokio::test]
    async fn adds_digest_header_to_request() {
        let client = setup_digest_client("http://example.com".to_owned());
        let method = http::Method::GET;
        let url = url::Url::parse("http://example.com").unwrap();
        // add digest manually so we don't make a request at this stage.
        client.update_auth_context("Digest realm=\"example.com\", qop=\"auth\", nonce=\"dcd98b7102dd2f0e8b11d0f600bfb0c093\", opaque=\"5ccc069c403ebaf9f0171e9517f40e41\"").await.unwrap();
        let builder = client.agent.request(method.clone(), url.as_str());
        let builder = client
            .apply_authentication(builder, &method, &url)
            .await
            .unwrap();
        let request = builder.build().unwrap();
        let headers = request.headers();
        let auth_header = headers.get("Authorization").unwrap().to_str().unwrap();
        assert!(auth_header.starts_with("Digest"));
    }

    #[tokio::test]
    async fn increments_nc_on_requests() {
        let client = setup_digest_client("http://example.com".to_owned());
        // add digest manually so we don't make a request at this stage.
        client.update_auth_context("Digest realm=\"example.com\", qop=\"auth\", nonce=\"dcd98b7102dd2f0e8b11d0f600bfb0c093\", opaque=\"5ccc069c403ebaf9f0171e9517f40e41\"").await.unwrap();
        let method = http::Method::GET;
        let url = url::Url::parse("http://example.com").unwrap();
        let builder = client.agent.request(method.clone(), url.as_str());
        let builder = client
            .apply_authentication(builder, &method, &url)
            .await
            .unwrap();
        let request = builder.build().unwrap();
        let headers = request.headers();
        let auth_header = headers.get("Authorization").unwrap().to_str().unwrap();
        assert!(auth_header.contains("nc=00000001"));

        let builder = client.agent.request(method.clone(), url.as_str());
        let builder = client
            .apply_authentication(builder, &method, &url)
            .await
            .unwrap();
        let request = builder.build().unwrap();
        let headers = request.headers();
        let auth_header = headers.get("Authorization").unwrap().to_str().unwrap();
        assert!(auth_header.contains("nc=00000002"));
    }

    #[tokio::test]
    async fn will_query_server_for_digest_auth_if_not_initialized() {
        let mock_server = MockServer::start().await;
        let server_digest_header = "Digest realm=\"example.com\", qop=\"auth\", nonce=\"dcd98b7102dd2f0e8b11d0f600bfb0c093\", opaque=\"5ccc069c403ebaf9f0171e9517f40e41\"";

        // just always pass for now.
        Mock::given(method("GET"))
            .and(header_exists("Authorization"))
            .respond_with(wiremock::ResponseTemplate::new(200))
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .respond_with(
                wiremock::ResponseTemplate::new(401)
                    .append_header("WWW-Authenticate", server_digest_header),
            )
            .expect(1)
            .mount(&mock_server)
            .await;
        println!("Running mock server at {}", mock_server.uri());
        let client = setup_digest_client(mock_server.uri());
        let result = client.get_raw("/").await;
        assert!(result.is_ok());
        mock_server.verify().await;
    }

    #[tokio::test]
    async fn digest_initialisation_will_match_the_method_and_url() {
        let mock_server = MockServer::start().await;
        let server_digest_header = "Digest realm=\"example.com\", qop=\"auth\", nonce=\"dcd98b7102dd2f0e8b11d0f600bfb0c093\", opaque=\"5ccc069c403ebaf9f0171e9517f40e41\"";

        // just always pass for now.
        Mock::given(method("PROPFIND"))
            .and(header_exists("Authorization"))
            .respond_with(wiremock::ResponseTemplate::new(200))
            .expect(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("PROPFIND"))
            .respond_with(
                wiremock::ResponseTemplate::new(401)
                    .append_header("WWW-Authenticate", server_digest_header),
            )
            .expect(1)
            .mount(&mock_server)
            .await;
        println!("Running mock server at {}", mock_server.uri());
        let client = setup_digest_client(mock_server.uri());
        let result = client.list_raw("/", Depth::Number(1)).await;
        assert!(result.is_ok());
        mock_server.verify().await;
    }

    #[tokio::test]
    async fn test_basic_auth() {
        let mock_server = MockServer::start().await;

        Mock::given(basic_auth("user", "password"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1..)
            .mount(&mock_server)
            .await;

        let client = ClientBuilder::new()
            .set_host(mock_server.uri())
            .set_auth(Auth::Basic("user".to_owned(), "password".to_owned()))
            .build()
            .unwrap();
        let response = client.get_raw("/").await.unwrap();
        assert_eq!(response.status().as_u16(), 200);
    }
}
