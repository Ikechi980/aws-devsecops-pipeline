mod common;

use reqwest::StatusCode;

#[tokio::test]
async fn health_endpoint_returns_ok() {
    let client = common::http_client();

    let response = client
        .get(common::HEALTH_URL)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert_eq!(body["status"], "healthy");
    assert!(body["connected_clients"].is_number());
}
