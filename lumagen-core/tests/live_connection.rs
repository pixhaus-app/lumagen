//! Live-network checks for the provider layer (ignored by default — run explicitly with
//! `cargo test -p lumagen-core --test live_connection -- --ignored`). These verify the whole
//! reqwest + rustls stack actually completes TLS requests, which a unit test can't.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods, clippy::print_stdout, clippy::panic)]

use lumagen_core::providers::{ProviderError, ProviderId, test_connection};

/// A bad key must come back as an HTTP auth error — reaching the API at all proves the
/// TLS handshake worked.
async fn assert_bad_key_yields_auth_error(provider: ProviderId, key: &str, models: &[String]) {
    match test_connection(provider, key, models).await {
        Err(ProviderError::Api { status, .. }) => {
            println!("got expected API error, status {status}");
            assert!(status == 401 || status == 403, "expected auth failure, got {status}");
        }
        other => panic!("expected Api auth error (proves TLS handshake worked), got: {other:?}"),
    }
}

#[tokio::test]
#[ignore = "hits the real network"]
async fn openrouter_test_connection_with_bad_key_returns_api_error() {
    assert_bad_key_yields_auth_error(ProviderId::OpenRouter, "sk-or-invalid-key-for-tls-check", &["x".to_string()]).await;
}

#[tokio::test]
#[ignore = "hits the real network"]
async fn fal_test_connection_with_bad_key_returns_api_error() {
    assert_bad_key_yields_auth_error(ProviderId::Fal, "invalid-key-for-tls-check", &["fal-ai/flux-pro/kontext".to_string()]).await;
}
