use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

const BASE_URL: &str = "http://127.0.0.1:9000/lambda-url/resources-api";

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Community {
    id: Uuid,
    name: String,
    yardi_org_id: Option<String>,
    yardi_api_key: Option<String>,
    yardi_api_secret: Option<String>,
    yardi_api_base_url: Option<String>,
    yardi_token_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: String,
    reason: String,
}

async fn create_test_community(client: &Client, name: &str) -> Uuid {
    let response = client
        .post(format!("{}/v1/communities", BASE_URL))
        .json(&json!({"name": name}))
        .send()
        .await
        .expect("Failed to create community");

    let community: Community = response.json().await.expect("Failed to parse community");
    community.id
}

async fn delete_community(client: &Client, id: Uuid) {
    let _ = client
        .delete(format!("{}/v1/communities/{}", BASE_URL, id))
        .send()
        .await;
}

#[tokio::test]
async fn test_create_community_success() {
    let client = Client::new();
    let name = format!("Test Community {}", Uuid::new_v4());

    let response = client
        .post(format!("{}/v1/communities", BASE_URL))
        .json(&json!({"name": name}))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::CREATED);

    let community: Community = response.json().await.expect("Failed to parse response");
    assert_eq!(community.name, name);
    assert_ne!(community.id, Uuid::nil());

    delete_community(&client, community.id).await;
}

#[tokio::test]
async fn test_create_community_missing_name() {
    let client = Client::new();

    let response = client
        .post(format!("{}/v1/communities", BASE_URL))
        .json(&json!({}))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_create_community_invalid_json() {
    let client = Client::new();

    let response = client
        .post(format!("{}/v1/communities", BASE_URL))
        .header("content-type", "application/json")
        .body("not valid json")
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_create_community_empty_name() {
    let client = Client::new();

    let response = client
        .post(format!("{}/v1/communities", BASE_URL))
        .json(&json!({"name": ""}))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_list_communities() {
    let client = Client::new();

    let id1 = create_test_community(&client, "List Test 1").await;
    let id2 = create_test_community(&client, "List Test 2").await;

    let response = client
        .get(format!("{}/v1/communities", BASE_URL))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    let communities: Vec<Community> = response.json().await.expect("Failed to parse response");
    assert!(communities.len() >= 2);

    delete_community(&client, id1).await;
    delete_community(&client, id2).await;
}

#[tokio::test]
async fn test_get_community_success() {
    let client = Client::new();
    let name = "Get Test Community";
    let id = create_test_community(&client, name).await;

    let response = client
        .get(format!("{}/v1/communities/{}", BASE_URL, id))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    let community: Community = response.json().await.expect("Failed to parse response");
    assert_eq!(community.id, id);
    assert_eq!(community.name, name);

    delete_community(&client, id).await;
}

#[tokio::test]
async fn test_get_community_not_found() {
    let client = Client::new();
    let fake_id = Uuid::new_v4();

    let response = client
        .get(format!("{}/v1/communities/{}", BASE_URL, fake_id))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let error: ErrorResponse = response.json().await.expect("Failed to parse error");
    assert!(!error.error.is_empty());

    assert!(!error.error.to_lowercase().contains("sql"));
    assert!(!error.error.to_lowercase().contains("database"));
}

#[tokio::test]
async fn test_get_community_invalid_uuid() {
    let client = Client::new();

    let response = client
        .get(format!("{}/v1/communities/not-a-uuid", BASE_URL))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_update_community_success() {
    let client = Client::new();
    let id = create_test_community(&client, "Original Name").await;

    let new_name = "Updated Name";
    let response = client
        .put(format!("{}/v1/communities/{}", BASE_URL, id))
        .json(&json!({"name": new_name}))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    let community: Community = response.json().await.expect("Failed to parse response");
    assert_eq!(community.id, id);
    assert_eq!(community.name, new_name);

    let get_response = client
        .get(format!("{}/v1/communities/{}", BASE_URL, id))
        .send()
        .await
        .expect("Failed to get community");

    let fetched: Community = get_response.json().await.unwrap();
    assert_eq!(fetched.name, new_name);

    delete_community(&client, id).await;
}

#[tokio::test]
async fn test_update_community_not_found() {
    let client = Client::new();
    let fake_id = Uuid::new_v4();

    let response = client
        .put(format!("{}/v1/communities/{}", BASE_URL, fake_id))
        .json(&json!({"name": "New Name"}))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_update_community_invalid_json() {
    let client = Client::new();
    let id = create_test_community(&client, "Test").await;

    let response = client
        .put(format!("{}/v1/communities/{}", BASE_URL, id))
        .header("content-type", "application/json")
        .body("invalid")
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    delete_community(&client, id).await;
}

#[tokio::test]
async fn test_delete_community_success() {
    let client = Client::new();
    let id = create_test_community(&client, "To Delete").await;

    let response = client
        .delete(format!("{}/v1/communities/{}", BASE_URL, id))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let get_response = client
        .get(format!("{}/v1/communities/{}", BASE_URL, id))
        .send()
        .await
        .expect("Failed to get community");

    assert_eq!(get_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_delete_community_not_found() {
    let client = Client::new();
    let fake_id = Uuid::new_v4();

    let response = client
        .delete(format!("{}/v1/communities/{}", BASE_URL, fake_id))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_delete_community_with_locations() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Community with Locations").await;

    let location_response = client
        .post(format!(
            "{}/v1/communities/{}/locations",
            BASE_URL, community_id
        ))
        .json(&json!({"name": "Test Location", "location_type": "apartment"}))
        .send()
        .await
        .expect("Failed to create location");

    assert_eq!(location_response.status(), StatusCode::CREATED);

    let delete_response = client
        .delete(format!("{}/v1/communities/{}", BASE_URL, community_id))
        .send()
        .await
        .expect("Failed to send delete request");

    assert_eq!(delete_response.status(), StatusCode::CONFLICT);

    let error: ErrorResponse = delete_response.json().await.expect("Failed to parse error");
    assert!(!error.error.is_empty());

    assert!(!error.error.contains("23503"));
    assert!(!error.error.to_lowercase().contains("foreign key"));

    let location_id: serde_json::Value = location_response.json().await.unwrap();
    let _ = client
        .delete(format!(
            "{}/v1/communities/{}/locations/{}",
            BASE_URL,
            community_id,
            location_id["id"].as_str().unwrap()
        ))
        .send()
        .await;

    delete_community(&client, community_id).await;
}

#[tokio::test]
async fn test_method_not_allowed() {
    let client = Client::new();

    // PATCH not supported on communities
    let response = client
        .patch(format!("{}/v1/communities", BASE_URL))
        .json(&json!({"name": "test"}))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn test_create_community_name_too_long() {
    let client = Client::new();

    let long_name = "a".repeat(1001);
    let response = client
        .post(format!("{}/v1/communities", BASE_URL))
        .json(&json!({"name": long_name}))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let error: ErrorResponse = response.json().await.expect("Failed to parse error");
    assert!(error.error.contains("maximum length"));
    assert_eq!(error.reason, "name_too_long");
}

#[tokio::test]
async fn test_update_community_name_too_long() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Test Community").await;

    let long_name = "a".repeat(1001);
    let response = client
        .put(format!("{}/v1/communities/{}", BASE_URL, community_id))
        .json(&json!({"name": long_name}))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let error: ErrorResponse = response.json().await.expect("Failed to parse error");
    assert!(error.error.contains("maximum length"));

    delete_community(&client, community_id).await;
}

// --- Yardi Integration Tests ---

#[tokio::test]
async fn test_create_community_with_yardi_fields() {
    let client = Client::new();
    let name = format!("Yardi Community {}", Uuid::new_v4());

    let response = client
        .post(format!("{}/v1/communities", BASE_URL))
        .json(&json!({
            "name": name,
            "yardi_org_id": "ORG123",
            "yardi_api_key": "API_KEY",
            "yardi_api_secret": "API_SECRET",
            "yardi_api_base_url": "https://api.example.com/fhir",
            "yardi_token_url": "https://api.example.com/oauth/token"
        }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::CREATED);

    let community: Community = response.json().await.expect("Failed to parse response");
    assert_eq!(community.name, name);
    assert_eq!(community.yardi_org_id, Some("ORG123".to_string()));
    assert_eq!(community.yardi_api_key, Some("API_KEY".to_string()));
    assert_eq!(community.yardi_api_secret, Some("API_SECRET".to_string()));
    assert_eq!(
        community.yardi_api_base_url,
        Some("https://api.example.com/fhir".to_string())
    );
    assert_eq!(
        community.yardi_token_url,
        Some("https://api.example.com/oauth/token".to_string())
    );

    delete_community(&client, community.id).await;
}

#[tokio::test]
async fn test_create_community_partial_yardi_fields_fails() {
    let client = Client::new();

    // Only org_id set
    let response = client
        .post(format!("{}/v1/communities", BASE_URL))
        .json(&json!({
            "name": "Test",
            "yardi_org_id": "ORG123"
        }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let error: ErrorResponse = response.json().await.expect("Failed to parse error");
    assert!(error.error.contains("all set or all unset"));
}

#[tokio::test]
async fn test_create_community_invalid_yardi_api_base_url_fails() {
    let client = Client::new();

    let response = client
        .post(format!("{}/v1/communities", BASE_URL))
        .json(&json!({
            "name": "Test",
            "yardi_org_id": "ORG123",
            "yardi_api_key": "API_KEY",
            "yardi_api_secret": "API_SECRET",
            "yardi_api_base_url": "not-a-url",
            "yardi_token_url": "https://api.example.com/oauth/token"
        }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let error: ErrorResponse = response.json().await.expect("Failed to parse error");
    assert_eq!(error.reason, "yardi_api_base_url_invalid");
}

#[tokio::test]
async fn test_create_community_yardi_api_base_url_with_query_fails() {
    let client = Client::new();

    let response = client
        .post(format!("{}/v1/communities", BASE_URL))
        .json(&json!({
            "name": "Test",
            "yardi_org_id": "ORG123",
            "yardi_api_key": "API_KEY",
            "yardi_api_secret": "API_SECRET",
            "yardi_api_base_url": "https://api.example.com/fhir?tenant=abc",
            "yardi_token_url": "https://api.example.com/oauth/token"
        }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let error: ErrorResponse = response.json().await.expect("Failed to parse error");
    assert_eq!(error.reason, "yardi_api_base_url_invalid");
}

#[tokio::test]
async fn test_create_community_invalid_yardi_token_url_fails() {
    let client = Client::new();

    let response = client
        .post(format!("{}/v1/communities", BASE_URL))
        .json(&json!({
            "name": "Test",
            "yardi_org_id": "ORG123",
            "yardi_api_key": "API_KEY",
            "yardi_api_secret": "API_SECRET",
            "yardi_api_base_url": "https://api.example.com/fhir",
            "yardi_token_url": "api.example.com/oauth/token"
        }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let error: ErrorResponse = response.json().await.expect("Failed to parse error");
    assert_eq!(error.reason, "yardi_token_url_invalid");
}

#[tokio::test]
async fn test_create_community_two_yardi_fields_fails() {
    let client = Client::new();

    // org_id and api_key set, but not api_secret
    let response = client
        .post(format!("{}/v1/communities", BASE_URL))
        .json(&json!({
            "name": "Test",
            "yardi_org_id": "ORG123",
            "yardi_api_key": "API_KEY"
        }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let error: ErrorResponse = response.json().await.expect("Failed to parse error");
    assert!(error.error.contains("all set or all unset"));
}

#[tokio::test]
async fn test_update_community_invalid_yardi_token_url_fails() {
    let client = Client::new();
    let id = create_test_community(&client, "No Yardi").await;

    let response = client
        .put(format!("{}/v1/communities/{}", BASE_URL, id))
        .json(&json!({
            "name": "With Yardi",
            "yardi_org_id": "ORG123",
            "yardi_api_key": "API_KEY",
            "yardi_api_secret": "API_SECRET",
            "yardi_api_base_url": "https://api.example.com/fhir",
            "yardi_token_url": "api.example.com/oauth/token"
        }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let error: ErrorResponse = response.json().await.expect("Failed to parse error");
    assert_eq!(error.reason, "yardi_token_url_invalid");

    delete_community(&client, id).await;
}

#[tokio::test]
async fn test_update_community_add_yardi_fields() {
    let client = Client::new();
    let id = create_test_community(&client, "No Yardi").await;

    let response = client
        .put(format!("{}/v1/communities/{}", BASE_URL, id))
        .json(&json!({
            "name": "With Yardi",
            "yardi_org_id": "ORG123",
            "yardi_api_key": "API_KEY",
            "yardi_api_secret": "API_SECRET",
            "yardi_api_base_url": "https://api.example.com/fhir",
            "yardi_token_url": "https://api.example.com/oauth/token"
        }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    let community: Community = response.json().await.expect("Failed to parse response");
    assert_eq!(community.yardi_org_id, Some("ORG123".to_string()));
    assert_eq!(
        community.yardi_api_base_url,
        Some("https://api.example.com/fhir".to_string())
    );
    assert_eq!(
        community.yardi_token_url,
        Some("https://api.example.com/oauth/token".to_string())
    );

    delete_community(&client, id).await;
}

#[tokio::test]
async fn test_update_community_remove_yardi_fields() {
    let client = Client::new();

    // Create community with Yardi fields
    let response = client
        .post(format!("{}/v1/communities", BASE_URL))
        .json(&json!({
            "name": "With Yardi",
            "yardi_org_id": "ORG123",
            "yardi_api_key": "API_KEY",
            "yardi_api_secret": "API_SECRET",
            "yardi_api_base_url": "https://api.example.com/fhir",
            "yardi_token_url": "https://api.example.com/oauth/token"
        }))
        .send()
        .await
        .expect("Failed to create community");

    let community: Community = response.json().await.expect("Failed to parse community");

    // Update to remove Yardi fields
    let response = client
        .put(format!("{}/v1/communities/{}", BASE_URL, community.id))
        .json(&json!({"name": "No Yardi"}))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    let updated: Community = response.json().await.expect("Failed to parse response");
    assert_eq!(updated.yardi_org_id, None);
    assert_eq!(updated.yardi_api_key, None);
    assert_eq!(updated.yardi_api_secret, None);
    assert_eq!(updated.yardi_api_base_url, None);
    assert_eq!(updated.yardi_token_url, None);

    delete_community(&client, community.id).await;
}
