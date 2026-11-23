//! The studio guards its local API with a key (BEZPIECZENSTWO-I-DEBIAN.md). The tests start it with a key of their
//! own and send it with every request.
#![allow(dead_code)]

/// Given to the studio started by a test through YUE_LOCAL_API_KEY.
pub const TEST_KEY: &str = "integration-test-local-api-key-0123456789";

/// A client that sends the test key with every request.
pub fn keyed() -> reqwest::ClientBuilder {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::AUTHORIZATION,
        reqwest::header::HeaderValue::from_str(&format!("Bearer {TEST_KEY}")).unwrap(),
    );
    reqwest::Client::builder().default_headers(headers)
}
