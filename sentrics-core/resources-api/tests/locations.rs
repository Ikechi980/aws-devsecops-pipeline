use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

const BASE_URL: &str = "http://127.0.0.1:9000/lambda-url/resources-api";

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Location {
    id: Uuid,
    community_id: Uuid,
    name: String,
    location_type: String,
    yardi_reference_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
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

async fn create_test_location(client: &Client, community_id: Uuid, name: &str) -> Uuid {
    let response = client
        .post(format!(
            "{}/v1/communities/{}/locations",
            BASE_URL, community_id
        ))
        .json(&json!({"name": name, "location_type": "apartment"}))
        .send()
        .await
        .expect("Failed to create location");

    let location: Location = response.json().await.expect("Failed to parse location");
    location.id
}

async fn cleanup_community(client: &Client, community_id: Uuid) {
    let _ = client
        .delete(format!("{}/v1/communities/{}", BASE_URL, community_id))
        .send()
        .await;
}

async fn cleanup_location(client: &Client, community_id: Uuid, location_id: Uuid) {
    let _ = client
        .delete(format!(
            "{}/v1/communities/{}/locations/{}",
            BASE_URL, community_id, location_id
        ))
        .send()
        .await;
}

#[tokio::test]
async fn test_create_location_success() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Location Test Community").await;
    let location_name = format!("Test Location {}", Uuid::new_v4());

    let response = client
        .post(format!(
            "{}/v1/communities/{}/locations",
            BASE_URL, community_id
        ))
        .json(&json!({"name": location_name, "location_type": "apartment"}))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::CREATED);

    let location: Location = response.json().await.expect("Failed to parse response");
    assert_eq!(location.name, location_name);
    assert_eq!(location.location_type, "apartment");
    assert_eq!(location.community_id, community_id);
    assert_ne!(location.id, Uuid::nil());

    cleanup_location(&client, community_id, location.id).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_create_location_community_not_found() {
    let client = Client::new();
    let fake_community_id = Uuid::new_v4();

    let response = client
        .post(format!(
            "{}/v1/communities/{}/locations",
            BASE_URL, fake_community_id
        ))
        .json(&json!({"name": "Test Location", "location_type": "apartment"}))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let error: ErrorResponse = response.json().await.expect("Failed to parse error");
    assert!(!error.error.is_empty());

    assert!(!error.error.contains("23503"));
}

#[tokio::test]
async fn test_create_location_missing_name() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Test Community").await;

    let response = client
        .post(format!(
            "{}/v1/communities/{}/locations",
            BASE_URL, community_id
        ))
        .json(&json!({"location_type": "apartment"}))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_create_location_invalid_community_uuid() {
    let client = Client::new();

    let response = client
        .post(format!("{}/v1/communities/not-a-uuid/locations", BASE_URL))
        .json(&json!({ "name": "Test" }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_create_location_empty_name() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Test Community").await;

    let response = client
        .post(format!(
            "{}/v1/communities/{}/locations",
            BASE_URL, community_id
        ))
        .json(&json!({"name": "", "location_type": "apartment"}))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_list_locations_in_community() {
    let client = Client::new();
    let community_id = create_test_community(&client, "List Test Community").await;

    let id1 = create_test_location(&client, community_id, "Location 1").await;
    let id2 = create_test_location(&client, community_id, "Location 2").await;

    let response = client
        .get(format!(
            "{}/v1/communities/{}/locations",
            BASE_URL, community_id
        ))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    let locations: Vec<Location> = response.json().await.expect("Failed to parse response");
    assert_eq!(locations.len(), 2);
    assert!(locations.iter().all(|l| l.community_id == community_id));

    cleanup_location(&client, community_id, id1).await;
    cleanup_location(&client, community_id, id2).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_list_locations_empty_community() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Empty Community").await;

    let response = client
        .get(format!(
            "{}/v1/communities/{}/locations",
            BASE_URL, community_id
        ))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    let locations: Vec<Location> = response.json().await.expect("Failed to parse response");
    assert_eq!(locations.len(), 0);

    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_list_locations_community_not_found() {
    let client = Client::new();
    let fake_id = Uuid::new_v4();

    let response = client
        .get(format!("{}/v1/communities/{}/locations", BASE_URL, fake_id))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_get_location_success() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Get Test Community").await;
    let location_name = "Get Test Location";
    let location_id = create_test_location(&client, community_id, location_name).await;

    let response = client
        .get(format!(
            "{}/v1/communities/{}/locations/{}",
            BASE_URL, community_id, location_id
        ))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    let location: Location = response.json().await.expect("Failed to parse response");
    assert_eq!(location.id, location_id);
    assert_eq!(location.community_id, community_id);
    assert_eq!(location.name, location_name);

    cleanup_location(&client, community_id, location_id).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_get_location_not_found() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Test Community").await;
    let fake_location_id = Uuid::new_v4();

    let response = client
        .get(format!(
            "{}/v1/communities/{}/locations/{}",
            BASE_URL, community_id, fake_location_id
        ))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_get_location_wrong_community() {
    let client = Client::new();
    let community1_id = create_test_community(&client, "Community 1").await;
    let community2_id = create_test_community(&client, "Community 2").await;
    let location_id = create_test_location(&client, community1_id, "Test Location").await;

    let response = client
        .get(format!(
            "{}/v1/communities/{}/locations/{}",
            BASE_URL, community2_id, location_id
        ))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_location(&client, community1_id, location_id).await;
    cleanup_community(&client, community1_id).await;
    cleanup_community(&client, community2_id).await;
}

#[tokio::test]
async fn test_update_location_success() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Update Test Community").await;
    let location_id = create_test_location(&client, community_id, "Original Name").await;

    let new_name = "Updated Location Name";
    let response = client
        .put(format!(
            "{}/v1/communities/{}/locations/{}",
            BASE_URL, community_id, location_id
        ))
        .json(&json!({"name": new_name, "location_type": "apartment"}))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    let location: Location = response.json().await.expect("Failed to parse response");
    assert_eq!(location.id, location_id);
    assert_eq!(location.name, new_name);
    assert_eq!(location.community_id, community_id);

    cleanup_location(&client, community_id, location_id).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_update_location_not_found() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Test Community").await;
    let fake_location_id = Uuid::new_v4();

    let response = client
        .put(format!(
            "{}/v1/communities/{}/locations/{}",
            BASE_URL, community_id, fake_location_id
        ))
        .json(&json!({"name": "New Name", "location_type": "apartment"}))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_update_location_wrong_community() {
    let client = Client::new();
    let community1_id = create_test_community(&client, "Community 1").await;
    let community2_id = create_test_community(&client, "Community 2").await;
    let location_id = create_test_location(&client, community1_id, "Test Location").await;

    let response = client
        .put(format!(
            "{}/v1/communities/{}/locations/{}",
            BASE_URL, community2_id, location_id
        ))
        .json(&json!({"name": "New Name", "location_type": "apartment"}))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_location(&client, community1_id, location_id).await;
    cleanup_community(&client, community1_id).await;
    cleanup_community(&client, community2_id).await;
}

#[tokio::test]
async fn test_delete_location_success() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Delete Test Community").await;
    let location_id = create_test_location(&client, community_id, "To Delete").await;

    let response = client
        .delete(format!(
            "{}/v1/communities/{}/locations/{}",
            BASE_URL, community_id, location_id
        ))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let get_response = client
        .get(format!(
            "{}/v1/communities/{}/locations/{}",
            BASE_URL, community_id, location_id
        ))
        .send()
        .await
        .expect("Failed to get location");

    assert_eq!(get_response.status(), StatusCode::NOT_FOUND);

    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_delete_location_not_found() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Test Community").await;
    let fake_location_id = Uuid::new_v4();

    let response = client
        .delete(format!(
            "{}/v1/communities/{}/locations/{}",
            BASE_URL, community_id, fake_location_id
        ))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_delete_location_with_residents() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Delete Test Community").await;
    let location_id = create_test_location(&client, community_id, "Location with Residents").await;

    let resident_response = client
        .post(format!(
            "{}/v1/communities/{}/residents",
            BASE_URL, community_id
        ))
        .json(&json!({
            "first_name": "Test",
            "last_name": "Resident",
            "location_id": location_id
        }))
        .send()
        .await
        .expect("Failed to create resident");

    assert_eq!(resident_response.status(), StatusCode::CREATED);

    let delete_response = client
        .delete(format!(
            "{}/v1/communities/{}/locations/{}",
            BASE_URL, community_id, location_id
        ))
        .send()
        .await
        .expect("Failed to send delete request");

    assert_eq!(delete_response.status(), StatusCode::CONFLICT);

    let error: ErrorResponse = delete_response.json().await.expect("Failed to parse error");
    assert!(!error.error.is_empty());

    assert!(!error.error.contains("23503"));

    let resident: serde_json::Value = resident_response.json().await.unwrap();
    let _ = client
        .delete(format!(
            "{}/v1/communities/{}/residents/{}",
            BASE_URL,
            community_id,
            resident["id"].as_str().unwrap()
        ))
        .send()
        .await;

    cleanup_location(&client, community_id, location_id).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_create_location_name_too_long() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Test Community").await;

    let long_name = "a".repeat(1001);
    let response = client
        .post(format!(
            "{}/v1/communities/{}/locations",
            BASE_URL, community_id
        ))
        .json(&json!({"name": long_name, "location_type": "apartment"}))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let error: ErrorResponse = response.json().await.expect("Failed to parse error");
    assert!(error.error.contains("maximum length"));

    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_update_location_name_too_long() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Test Community").await;
    let location_id = create_test_location(&client, community_id, "Test Location").await;

    let long_name = "a".repeat(1001);
    let response = client
        .put(format!(
            "{}/v1/communities/{}/locations/{}",
            BASE_URL, community_id, location_id
        ))
        .json(&json!({"name": long_name, "location_type": "apartment"}))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let error: ErrorResponse = response.json().await.expect("Failed to parse error");
    assert!(error.error.contains("maximum length"));

    cleanup_location(&client, community_id, location_id).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_create_location_missing_location_type() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Test Community").await;

    let response = client
        .post(format!(
            "{}/v1/communities/{}/locations",
            BASE_URL, community_id
        ))
        .json(&json!({"name": "Test Location"}))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let error: ErrorResponse = response.json().await.expect("Failed to parse error");
    assert_eq!(error.reason, "missing_location_type");
    assert!(error.error.contains("location_type is required"));

    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_update_location_missing_location_type() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Test Community").await;
    let location_id = create_test_location(&client, community_id, "Test Location").await;

    let response = client
        .put(format!(
            "{}/v1/communities/{}/locations/{}",
            BASE_URL, community_id, location_id
        ))
        .json(&json!({"name": "Updated Name"}))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let error: ErrorResponse = response.json().await.expect("Failed to parse error");
    assert_eq!(error.reason, "missing_location_type");
    assert!(error.error.contains("location_type is required"));

    cleanup_location(&client, community_id, location_id).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_location_type_serializes_as_snake_case() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Test Community").await;

    let response = client
        .post(format!(
            "{}/v1/communities/{}/locations",
            BASE_URL, community_id
        ))
        .json(&json!({"name": "Test Location", "location_type": "apartment"}))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::CREATED);

    let location: Location = response.json().await.expect("Failed to parse location");
    assert_eq!(location.location_type, "apartment");

    // Verify it's returned in lists as well
    let list_response = client
        .get(format!(
            "{}/v1/communities/{}/locations",
            BASE_URL, community_id
        ))
        .send()
        .await
        .expect("Failed to get locations");

    let locations: Vec<Location> = list_response
        .json()
        .await
        .expect("Failed to parse locations");
    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0].location_type, "apartment");

    cleanup_location(&client, community_id, location.id).await;
    cleanup_community(&client, community_id).await;
}

// --- Yardi Reference ID Tests ---

async fn create_yardi_community(client: &Client, name: &str) -> Uuid {
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
        .expect("Failed to create Yardi community");

    let community: Community = response.json().await.expect("Failed to parse community");
    community.id
}

#[tokio::test]
async fn test_create_location_with_yardi_reference_id() {
    let client = Client::new();
    let community_id = create_yardi_community(&client, "Yardi Location Test").await;

    let response = client
        .post(format!(
            "{}/v1/communities/{}/locations",
            BASE_URL, community_id
        ))
        .json(&json!({
            "name": "Yardi Location",
            "location_type": "apartment",
            "yardi_reference_id": "LOC-001"
        }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::CREATED);

    let location: Location = response.json().await.expect("Failed to parse response");
    assert_eq!(location.yardi_reference_id, Some("LOC-001".to_string()));

    cleanup_location(&client, community_id, location.id).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_create_location_yardi_reference_id_without_integration_fails() {
    let client = Client::new();
    let community_id = create_test_community(&client, "No Yardi Community").await;

    let response = client
        .post(format!(
            "{}/v1/communities/{}/locations",
            BASE_URL, community_id
        ))
        .json(&json!({
            "name": "Test Location",
            "location_type": "apartment",
            "yardi_reference_id": "LOC-001"
        }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::CONFLICT);

    let error: ErrorResponse = response.json().await.expect("Failed to parse error");
    assert!(error.error.contains("Yardi integration"));
    assert_eq!(error.reason, "yardi_integration_required");

    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_location_yardi_reference_id_unique_in_community() {
    let client = Client::new();
    let community_id = create_yardi_community(&client, "Yardi Unique Test").await;

    // Create first location with yardi_reference_id
    let location1_id = create_test_location(&client, community_id, "Location 1").await;

    // Update it to have a Yardi reference ID
    client
        .put(format!(
            "{}/v1/communities/{}/locations/{}",
            BASE_URL, community_id, location1_id
        ))
        .json(&json!({
            "name": "Location 1",
            "location_type": "apartment",
            "yardi_reference_id": "LOC-001"
        }))
        .send()
        .await
        .expect("Failed to update location");

    // Try to create second location with same yardi_reference_id
    let response = client
        .post(format!(
            "{}/v1/communities/{}/locations",
            BASE_URL, community_id
        ))
        .json(&json!({
            "name": "Location 2",
            "location_type": "apartment",
            "yardi_reference_id": "LOC-001"
        }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::CONFLICT);

    let error: ErrorResponse = response.json().await.expect("Failed to parse error");
    assert!(error.error.contains("already exists"));

    cleanup_location(&client, community_id, location1_id).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_cannot_unset_yardi_integration_with_location_reference() {
    let client = Client::new();
    let community_id = create_yardi_community(&client, "Yardi Unset Test").await;

    // Create location with yardi_reference_id
    let response = client
        .post(format!(
            "{}/v1/communities/{}/locations",
            BASE_URL, community_id
        ))
        .json(&json!({
            "name": "Yardi Location",
            "location_type": "apartment",
            "yardi_reference_id": "LOC-001"
        }))
        .send()
        .await
        .expect("Failed to create location");

    let location: Location = response.json().await.expect("Failed to parse location");

    // Try to remove Yardi integration from community
    let response = client
        .put(format!("{}/v1/communities/{}", BASE_URL, community_id))
        .json(&json!({"name": "No Yardi"}))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::CONFLICT);

    let error: ErrorResponse = response.json().await.expect("Failed to parse error");
    assert!(error.error.contains("Cannot unset Yardi integration"));

    cleanup_location(&client, community_id, location.id).await;
    cleanup_community(&client, community_id).await;
}
