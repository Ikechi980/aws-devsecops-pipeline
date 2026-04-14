mod common;

use serde_json::Value;

fn ensure_header_value(ensure_id: &str) -> (String, String) {
    ("x-ensure-community-id".to_string(), ensure_id.to_string())
}

#[tokio::test]
async fn missing_ensure_header_returns_400() {
    let url = format!("{}/v1/core/community", common::base_url());

    let response = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .expect("core community request failed");

    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    let payload: Value = response.json().await.expect("invalid json payload");
    assert_eq!(
        payload["reason"],
        Value::String("ensure_community_id_missing".to_string())
    );
}

#[tokio::test]
async fn empty_ensure_header_returns_400() {
    let url = format!("{}/v1/core/community", common::base_url());

    let response = reqwest::Client::new()
        .get(url)
        .header("x-ensure-community-id", "")
        .send()
        .await
        .expect("core community request failed");

    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    let payload: Value = response.json().await.expect("invalid json payload");
    assert_eq!(
        payload["reason"],
        Value::String("ensure_community_id_missing".to_string())
    );
}

#[tokio::test]
async fn ensure360_ems_error_returns_502() {
    let url = format!("{}/v1/core/community", common::base_url());
    let (header, value) = ensure_header_value("ems-error");

    let response = reqwest::Client::new()
        .get(url)
        .header(header, value)
        .send()
        .await
        .expect("core community request failed");

    assert_eq!(response.status(), reqwest::StatusCode::BAD_GATEWAY);
    let payload: Value = response.json().await.expect("invalid json payload");
    assert_eq!(
        payload["reason"],
        Value::String("ensure360_ems_error".to_string())
    );
}

#[tokio::test]
async fn ensure360_ems_invalid_response_returns_502() {
    let url = format!("{}/v1/core/community", common::base_url());
    let (header, value) = ensure_header_value("ems-invalid");

    let response = reqwest::Client::new()
        .get(url)
        .header(header, value)
        .send()
        .await
        .expect("core community request failed");

    assert_eq!(response.status(), reqwest::StatusCode::BAD_GATEWAY);
    let payload: Value = response.json().await.expect("invalid json payload");
    assert_eq!(
        payload["reason"],
        Value::String("ensure360_ems_invalid_response".to_string())
    );
}

#[tokio::test]
async fn core_resources_error_returns_502() {
    let url = format!("{}/v1/core/community", common::base_url());
    let (header, value) = ensure_header_value("core-error");

    let response = reqwest::Client::new()
        .get(url)
        .header(header, value)
        .send()
        .await
        .expect("core community request failed");

    assert_eq!(response.status(), reqwest::StatusCode::BAD_GATEWAY);
    let payload: Value = response.json().await.expect("invalid json payload");
    assert_eq!(
        payload["reason"],
        Value::String("core_resources_error".to_string())
    );
}

#[tokio::test]
async fn core_resources_invalid_response_returns_502() {
    let url = format!("{}/v1/core/community", common::base_url());
    let (header, value) = ensure_header_value("core-invalid");

    let response = reqwest::Client::new()
        .get(url)
        .header(header, value)
        .send()
        .await
        .expect("core community request failed");

    assert_eq!(response.status(), reqwest::StatusCode::BAD_GATEWAY);
    let payload: Value = response.json().await.expect("invalid json payload");
    assert_eq!(
        payload["reason"],
        Value::String("core_resources_invalid_response".to_string())
    );
}

#[tokio::test]
async fn core_resource_not_found_returns_404() {
    let url = format!("{}/v1/core/community", common::base_url());
    let (header, value) = ensure_header_value("core-missing");

    let response = reqwest::Client::new()
        .get(url)
        .header(header, value)
        .send()
        .await
        .expect("core community request failed");

    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);
    let payload: Value = response.json().await.expect("invalid json payload");
    assert_eq!(
        payload["reason"],
        Value::String("core_resource_not_found".to_string())
    );
}

#[tokio::test]
async fn core_resident_photo_not_found_returns_404() {
    let url = format!(
        "{}/v1/core/residents/{}/photo",
        common::base_url(),
        "eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee"
    );
    let (header, value) = ensure_header_value("alpha");

    let response = reqwest::Client::new()
        .get(url)
        .header(header, value)
        .send()
        .await
        .expect("core resident photo request failed");

    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);
    let payload: Value = response.json().await.expect("invalid json payload");
    assert_eq!(
        payload["reason"],
        Value::String("core_resource_not_found".to_string())
    );
}

#[tokio::test]
async fn core_resident_photo_core_resources_error_returns_502() {
    let url = format!(
        "{}/v1/core/residents/{}/photo",
        common::base_url(),
        "dddddddd-dddd-dddd-dddd-dddddddddddd"
    );
    let (header, value) = ensure_header_value("core-error");

    let response = reqwest::Client::new()
        .get(url)
        .header(header, value)
        .send()
        .await
        .expect("core resident photo request failed");

    assert_eq!(response.status(), reqwest::StatusCode::BAD_GATEWAY);
    let payload: Value = response.json().await.expect("invalid json payload");
    assert_eq!(
        payload["reason"],
        Value::String("core_resources_error".to_string())
    );
}
