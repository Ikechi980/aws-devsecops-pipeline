mod common;

use chrono::Utc;
use serde_json::Value;

fn ensure_header_value(ensure_id: &str) -> (String, String) {
    ("x-ensure-community-id".to_string(), ensure_id.to_string())
}

#[tokio::test]
async fn events_requires_payload_types() {
    let url = format!("{}/v1/events", common::base_url());
    let (header, value) = ensure_header_value("alpha");

    let response = reqwest::Client::new()
        .get(&url)
        .header(header, value)
        .send()
        .await
        .expect("events request failed");

    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    let payload: Value = response.json().await.expect("invalid json payload");
    assert_eq!(
        payload["reason"],
        Value::String("payload_types_missing".to_string())
    );
}

#[tokio::test]
async fn events_returns_array_with_valid_query() {
    let url = format!(
        "{}/v1/events?payloadTypes=ptt-transmission,ptt-user-mute&limit=10",
        common::base_url()
    );
    let (header, value) = ensure_header_value("alpha");

    let response = reqwest::Client::new()
        .get(&url)
        .header(header, value)
        .send()
        .await
        .expect("events request failed");

    assert!(response.status().is_success());
    let payload: Value = response.json().await.expect("invalid json payload");
    assert!(payload.is_array());
}

#[tokio::test]
async fn events_accepts_date_filters() {
    let after_date = Utc::now()
        .checked_sub_signed(chrono::Duration::days(7))
        .unwrap()
        .to_rfc3339();
    let before_date = Utc::now().to_rfc3339();

    let url = format!(
        "{}/v1/events?payloadTypes=ptt-transmission&afterDate={}&beforeDate={}&limit=5",
        common::base_url(),
        urlencoding::encode(&after_date),
        urlencoding::encode(&before_date)
    );
    let (header, value) = ensure_header_value("alpha");

    let response = reqwest::Client::new()
        .get(&url)
        .header(header, value)
        .send()
        .await
        .expect("events request failed");

    assert!(response.status().is_success());
    let payload: Value = response.json().await.expect("invalid json payload");
    assert!(payload.is_array());
}

#[tokio::test]
async fn events_respects_limit() {
    let url = format!(
        "{}/v1/events?payloadTypes=ptt-transmission&limit=3",
        common::base_url()
    );
    let (header, value) = ensure_header_value("alpha");

    let response = reqwest::Client::new()
        .get(&url)
        .header(header, value)
        .send()
        .await
        .expect("events request failed");

    assert!(response.status().is_success());
    let payload: Value = response.json().await.expect("invalid json payload");
    let arr = payload.as_array().expect("expected array");
    assert!(arr.len() <= 3);
}

#[tokio::test]
async fn events_handles_empty_payload_types() {
    let url = format!("{}/v1/events?payloadTypes=", common::base_url());
    let (header, value) = ensure_header_value("alpha");

    let response = reqwest::Client::new()
        .get(&url)
        .header(header, value)
        .send()
        .await
        .expect("events request failed");

    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn events_rejects_invalid_after_date() {
    let url = format!(
        "{}/v1/events?payloadTypes=test&afterDate=not-a-date",
        common::base_url()
    );
    let (header, value) = ensure_header_value("alpha");

    let response = reqwest::Client::new()
        .get(&url)
        .header(header, value)
        .send()
        .await
        .expect("events request failed");

    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn events_rejects_invalid_before_date() {
    let url = format!(
        "{}/v1/events?payloadTypes=test&beforeDate=invalid",
        common::base_url()
    );
    let (header, value) = ensure_header_value("alpha");

    let response = reqwest::Client::new()
        .get(&url)
        .header(header, value)
        .send()
        .await
        .expect("events request failed");

    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn events_allows_optional_date_filters() {
    let url = format!(
        "{}/v1/events?payloadTypes=device-info&limit=5",
        common::base_url()
    );
    let (header, value) = ensure_header_value("alpha");

    let response = reqwest::Client::new()
        .get(&url)
        .header(header, value)
        .send()
        .await
        .expect("events request failed");

    assert!(response.status().is_success());
    let payload: Value = response.json().await.expect("invalid json payload");
    assert!(payload.is_array());
}

#[tokio::test]
async fn events_returns_only_matching_community() {
    let url = format!(
        "{}/v1/events?payloadTypes=device-info&limit=100",
        common::base_url()
    );
    let (header, value) = ensure_header_value("alpha");

    let response = reqwest::Client::new()
        .get(&url)
        .header(header, value)
        .send()
        .await
        .expect("events request failed");

    assert!(response.status().is_success());
    let payload: Value = response.json().await.expect("invalid json payload");
    let arr = payload.as_array().expect("expected array");

    // All returned events should be for the "alpha" community
    for event in arr {
        assert_eq!(
            event["communityId"],
            Value::String("alpha".to_string()),
            "Event should belong to alpha community"
        );
    }
}

#[tokio::test]
async fn events_filters_by_payload_types() {
    let url = format!(
        "{}/v1/events?payloadTypes=device-info&limit=100",
        common::base_url()
    );
    let (header, value) = ensure_header_value("alpha");

    let response = reqwest::Client::new()
        .get(&url)
        .header(header, value)
        .send()
        .await
        .expect("events request failed");

    assert!(response.status().is_success());
    let payload: Value = response.json().await.expect("invalid json payload");
    let arr = payload.as_array().expect("expected array");

    // All returned events should have payloadType "device-info"
    for event in arr {
        assert_eq!(
            event["payloadType"],
            Value::String("device-info".to_string()),
            "Event should have device-info payload type"
        );
    }
}
