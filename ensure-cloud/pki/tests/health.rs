//! End-to-end tests for GET /v1/health endpoint.
//!
//! These tests require the full development environment to be running:
//!   Terminal 1: ./scripts/dev.sh run
//!   Terminal 2: cargo test

mod common;

use reqwest::StatusCode;

#[tokio::test]
async fn health_returns_ok() {
    let client = common::client();

    let response = client
        .get(format!("{}/v1/health", common::BASE_URL))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn post_to_health_returns_405() {
    let client = common::client();

    let response = client
        .post(format!("{}/v1/health", common::BASE_URL))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}
