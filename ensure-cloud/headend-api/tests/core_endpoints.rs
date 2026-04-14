mod common;

use serde_json::Value;

fn ensure_header_value(ensure_id: &str) -> (String, String) {
    ("x-ensure-community-id".to_string(), ensure_id.to_string())
}

#[tokio::test]
async fn core_community_returns_expected_fields() {
    let url = format!("{}/v1/core/community", common::base_url());
    let (header, value) = ensure_header_value("alpha");

    let response = reqwest::Client::new()
        .get(url)
        .header(header, value)
        .send()
        .await
        .expect("core community request failed");

    assert!(response.status().is_success());
    let payload: Value = response.json().await.expect("invalid json payload");
    assert_eq!(
        payload["id"],
        Value::String("11111111-1111-1111-1111-111111111111".to_string())
    );
    assert_eq!(
        payload["yardi_api_base_url"],
        Value::String("https://api.alpha.example.com/fhir".to_string())
    );
    assert_eq!(
        payload["yardi_token_url"],
        Value::String("https://api.alpha.example.com/oauth/token".to_string())
    );
}

#[tokio::test]
async fn core_locations_returns_array() {
    let url = format!("{}/v1/core/locations", common::base_url());
    let (header, value) = ensure_header_value("alpha");

    let response = reqwest::Client::new()
        .get(url)
        .header(header, value)
        .send()
        .await
        .expect("core locations request failed");

    assert!(response.status().is_success());
    let payload: Value = response.json().await.expect("invalid json payload");
    assert!(payload.is_array());
}

#[tokio::test]
async fn core_residents_returns_array() {
    let url = format!("{}/v1/core/residents", common::base_url());
    let (header, value) = ensure_header_value("alpha");

    let response = reqwest::Client::new()
        .get(url)
        .header(header, value)
        .send()
        .await
        .expect("core residents request failed");

    assert!(response.status().is_success());
    let payload: Value = response.json().await.expect("invalid json payload");
    assert!(payload.is_array());
    let residents = payload.as_array().expect("expected resident array");
    assert!(!residents.is_empty());
    for resident in residents {
        assert!(
            resident.get("first_name").is_some(),
            "resident payload must include first_name field"
        );
        assert!(
            resident.get("last_name").is_some(),
            "resident payload must include last_name field"
        );
        assert!(
            resident.get("name").is_none(),
            "resident payload must not include legacy name field"
        );
        assert!(
            resident.get("photo").is_some(),
            "resident payload must include photo field"
        );
    }
}

#[tokio::test]
async fn core_resident_photo_returns_bytes_and_headers() {
    let url = format!(
        "{}/v1/core/residents/{}/photo",
        common::base_url(),
        "dddddddd-dddd-dddd-dddd-dddddddddddd"
    );
    let (header, value) = ensure_header_value("alpha");

    let response = reqwest::Client::new()
        .get(url)
        .header(header, value)
        .send()
        .await
        .expect("core resident photo request failed");

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some("image/png")
    );
    assert_eq!(
        response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok()),
        Some("\"sha256:mock-alpha-resident-photo\"")
    );
    assert!(
        response
            .headers()
            .get(reqwest::header::LAST_MODIFIED)
            .is_some()
    );

    let body = response.bytes().await.expect("failed to read photo body");
    assert_eq!(body.as_ref(), &[0x89, 0x50, 0x4e, 0x47, 1, 2, 3, 4, 5]);
}

#[tokio::test]
async fn core_resident_photo_if_none_match_returns_304() {
    let url = format!(
        "{}/v1/core/residents/{}/photo",
        common::base_url(),
        "dddddddd-dddd-dddd-dddd-dddddddddddd"
    );
    let (header, value) = ensure_header_value("alpha");

    let response = reqwest::Client::new()
        .get(url)
        .header(header, value)
        .header(
            reqwest::header::IF_NONE_MATCH,
            "\"sha256:mock-alpha-resident-photo\"",
        )
        .send()
        .await
        .expect("core resident photo conditional request failed");

    assert_eq!(response.status(), reqwest::StatusCode::NOT_MODIFIED);
    assert_eq!(
        response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok()),
        Some("\"sha256:mock-alpha-resident-photo\"")
    );
    let body = response
        .bytes()
        .await
        .expect("failed to read response body");
    assert!(body.is_empty());
}

#[tokio::test]
async fn missing_core_mapping_returns_404() {
    let url = format!("{}/v1/core/community", common::base_url());
    let (header, value) = ensure_header_value("gamma");

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
        Value::String("core_community_mapping_missing".to_string())
    );
}
