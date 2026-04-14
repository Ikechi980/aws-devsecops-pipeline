use reqwest::{Client, StatusCode, header};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

const BASE_URL: &str = "http://127.0.0.1:9000/lambda-url/resources-api";

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Resident {
    id: Uuid,
    location_id: Uuid,
    community_id: Uuid,
    first_name: String,
    last_name: String,
    yardi_reference_id: Option<String>,
    photo: Option<ResidentPhotoMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct ResidentPhotoMetadata {
    etag: String,
    content_type: String,
    size_bytes: i64,
    updated_at: String,
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
#[allow(dead_code)]
struct Location {
    id: Uuid,
    community_id: Uuid,
    name: String,
    location_type: String,
    yardi_reference_id: Option<String>,
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

async fn create_test_resident(
    client: &Client,
    community_id: Uuid,
    location_id: Uuid,
    name: &str,
) -> Uuid {
    let (first_name, last_name) = split_name(name);
    let response = client
        .post(format!(
            "{}/v1/communities/{}/residents",
            BASE_URL, community_id
        ))
        .json(&json!({
            "first_name": first_name,
            "last_name": last_name,
            "location_id": location_id
        }))
        .send()
        .await
        .expect("Failed to create resident");

    let resident: Resident = response.json().await.expect("Failed to parse resident");
    resident.id
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

async fn cleanup_resident(client: &Client, community_id: Uuid, resident_id: Uuid) {
    let _ = client
        .delete(format!(
            "{}/v1/communities/{}/residents/{}",
            BASE_URL, community_id, resident_id
        ))
        .send()
        .await;
}

fn resident_photo_url(community_id: Uuid, resident_id: Uuid) -> String {
    format!(
        "{}/v1/communities/{}/residents/{}/photo",
        BASE_URL, community_id, resident_id
    )
}

fn split_name(name: &str) -> (String, String) {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return ("".to_string(), "".to_string());
    }

    if let Some((first, last)) = trimmed.rsplit_once(' ') {
        return (first.to_string(), last.to_string());
    }

    ("".to_string(), trimmed.to_string())
}

fn full_name(resident: &Resident) -> String {
    if resident.first_name.is_empty() {
        resident.last_name.clone()
    } else {
        format!("{} {}", resident.first_name, resident.last_name)
    }
}

#[tokio::test]
async fn test_create_resident_success() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Resident Test Community").await;
    let location_id = create_test_location(&client, community_id, "Test Location").await;
    let resident_name = format!("Test Resident {}", Uuid::new_v4());
    let (first_name, last_name) = split_name(&resident_name);

    let response = client
        .post(format!(
            "{}/v1/communities/{}/residents",
            BASE_URL, community_id
        ))
        .json(&json!({
            "first_name": first_name,
            "last_name": last_name,
            "location_id": location_id
        }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::CREATED);

    let resident: Resident = response.json().await.expect("Failed to parse response");
    assert_eq!(full_name(&resident), resident_name);
    assert_eq!(resident.location_id, location_id);
    assert_ne!(resident.id, Uuid::nil());

    cleanup_resident(&client, community_id, resident.id).await;
    cleanup_location(&client, community_id, location_id).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_create_resident_location_not_found() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Test Community").await;
    let fake_location_id = Uuid::new_v4();

    let response = client
        .post(format!(
            "{}/v1/communities/{}/residents",
            BASE_URL, community_id
        ))
        .json(&json!({
            "first_name": "Test",
            "last_name": "Resident",
            "location_id": fake_location_id
        }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let error: ErrorResponse = response.json().await.expect("Failed to parse error");
    assert!(!error.error.is_empty());

    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_create_resident_location_in_different_community() {
    let client = Client::new();
    let community1_id = create_test_community(&client, "Community 1").await;
    let community2_id = create_test_community(&client, "Community 2").await;
    let location_id = create_test_location(&client, community1_id, "Location in Community 1").await;

    // Try to create resident in community2 with location from community1
    let response = client
        .post(format!(
            "{}/v1/communities/{}/residents",
            BASE_URL, community2_id
        ))
        .json(&json!({
            "first_name": "Test",
            "last_name": "Resident",
            "location_id": location_id
        }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let error: ErrorResponse = response.json().await.expect("Failed to parse error");
    assert!(!error.error.is_empty());

    assert!(!error.error.contains("23503"));

    cleanup_location(&client, community1_id, location_id).await;
    cleanup_community(&client, community1_id).await;
    cleanup_community(&client, community2_id).await;
}

#[tokio::test]
async fn test_create_resident_missing_first_name() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Test Community").await;
    let location_id = create_test_location(&client, community_id, "Test Location").await;

    let response = client
        .post(format!(
            "{}/v1/communities/{}/residents",
            BASE_URL, community_id
        ))
        .json(&json!({"last_name": "Resident", "location_id": location_id}))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup_location(&client, community_id, location_id).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_create_resident_empty_first_name_allowed() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Test Community").await;
    let location_id = create_test_location(&client, community_id, "Test Location").await;

    let response = client
        .post(format!(
            "{}/v1/communities/{}/residents",
            BASE_URL, community_id
        ))
        .json(&json!({"first_name": "", "last_name": "Resident", "location_id": location_id}))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::CREATED);
    let resident: Resident = response.json().await.expect("Failed to parse response");
    assert_eq!(resident.first_name, "");
    assert_eq!(resident.last_name, "Resident");

    cleanup_resident(&client, community_id, resident.id).await;
    cleanup_location(&client, community_id, location_id).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_create_resident_missing_location_id() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Test Community").await;

    let response = client
        .post(format!(
            "{}/v1/communities/{}/residents",
            BASE_URL, community_id
        ))
        .json(&json!({"first_name": "Test", "last_name": "Resident"}))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let error: ErrorResponse = response.json().await.expect("Failed to parse error");
    assert_eq!(error.reason, "location_id_required");

    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_create_resident_empty_last_name() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Test Community").await;
    let location_id = create_test_location(&client, community_id, "Test Location").await;

    let response = client
        .post(format!(
            "{}/v1/communities/{}/residents",
            BASE_URL, community_id
        ))
        .json(&json!({"first_name": "Test", "last_name": "", "location_id": location_id}))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup_location(&client, community_id, location_id).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_list_residents_in_community() {
    let client = Client::new();
    let community_id = create_test_community(&client, "List Test Community").await;
    let location1_id = create_test_location(&client, community_id, "Location 1").await;
    let location2_id = create_test_location(&client, community_id, "Location 2").await;

    let id1 = create_test_resident(&client, community_id, location1_id, "Resident 1").await;
    let id2 = create_test_resident(&client, community_id, location2_id, "Resident 2").await;
    let id3 = create_test_resident(&client, community_id, location1_id, "Resident 3").await;

    let response = client
        .get(format!(
            "{}/v1/communities/{}/residents",
            BASE_URL, community_id
        ))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    let residents: Vec<Resident> = response.json().await.expect("Failed to parse response");
    assert_eq!(residents.len(), 3);

    for resident in &residents {
        assert!(resident.location_id == location1_id || resident.location_id == location2_id);
    }

    cleanup_resident(&client, community_id, id1).await;
    cleanup_resident(&client, community_id, id2).await;
    cleanup_resident(&client, community_id, id3).await;
    cleanup_location(&client, community_id, location1_id).await;
    cleanup_location(&client, community_id, location2_id).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_list_residents_empty_community() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Empty Community").await;

    let response = client
        .get(format!(
            "{}/v1/communities/{}/residents",
            BASE_URL, community_id
        ))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    let residents: Vec<Resident> = response.json().await.expect("Failed to parse response");
    assert_eq!(residents.len(), 0);

    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_list_residents_community_not_found() {
    let client = Client::new();
    let fake_id = Uuid::new_v4();

    let response = client
        .get(format!("{}/v1/communities/{}/residents", BASE_URL, fake_id))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_get_resident_success() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Get Test Community").await;
    let location_id = create_test_location(&client, community_id, "Test Location").await;
    let resident_name = "Get Test Resident";
    let resident_id = create_test_resident(&client, community_id, location_id, resident_name).await;

    let response = client
        .get(format!(
            "{}/v1/communities/{}/residents/{}",
            BASE_URL, community_id, resident_id
        ))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    let resident: Resident = response.json().await.expect("Failed to parse response");
    assert_eq!(resident.id, resident_id);
    assert_eq!(resident.location_id, location_id);
    assert_eq!(full_name(&resident), resident_name);

    cleanup_resident(&client, community_id, resident_id).await;
    cleanup_location(&client, community_id, location_id).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_get_resident_not_found() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Test Community").await;
    let fake_resident_id = Uuid::new_v4();

    let response = client
        .get(format!(
            "{}/v1/communities/{}/residents/{}",
            BASE_URL, community_id, fake_resident_id
        ))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_get_resident_wrong_community() {
    let client = Client::new();
    let community1_id = create_test_community(&client, "Community 1").await;
    let community2_id = create_test_community(&client, "Community 2").await;
    let location_id = create_test_location(&client, community1_id, "Test Location").await;
    let resident_id =
        create_test_resident(&client, community1_id, location_id, "Test Resident").await;

    let response = client
        .get(format!(
            "{}/v1/communities/{}/residents/{}",
            BASE_URL, community2_id, resident_id
        ))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_resident(&client, community1_id, resident_id).await;
    cleanup_location(&client, community1_id, location_id).await;
    cleanup_community(&client, community1_id).await;
    cleanup_community(&client, community2_id).await;
}

#[tokio::test]
async fn test_update_resident_success() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Update Test Community").await;
    let location_id = create_test_location(&client, community_id, "Test Location").await;
    let resident_id =
        create_test_resident(&client, community_id, location_id, "Original Name").await;

    let new_name = "Updated Resident Name";
    let (new_first_name, new_last_name) = split_name(new_name);
    let response = client
        .put(format!(
            "{}/v1/communities/{}/residents/{}",
            BASE_URL, community_id, resident_id
        ))
        .json(&json!({
            "first_name": new_first_name,
            "last_name": new_last_name,
            "location_id": location_id
        }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    let resident: Resident = response.json().await.expect("Failed to parse response");
    assert_eq!(resident.id, resident_id);
    assert_eq!(full_name(&resident), new_name);
    assert_eq!(resident.location_id, location_id);

    cleanup_resident(&client, community_id, resident_id).await;
    cleanup_location(&client, community_id, location_id).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_update_resident_move_to_different_location() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Update Test Community").await;
    let location1_id = create_test_location(&client, community_id, "Location 1").await;
    let location2_id = create_test_location(&client, community_id, "Location 2").await;
    let resident_id =
        create_test_resident(&client, community_id, location1_id, "Test Resident").await;

    // Move resident to location2
    let response = client
        .put(format!(
            "{}/v1/communities/{}/residents/{}",
            BASE_URL, community_id, resident_id
        ))
        .json(&json!({"first_name": "Test", "last_name": "Resident", "location_id": location2_id}))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    let resident: Resident = response.json().await.expect("Failed to parse response");
    assert_eq!(resident.location_id, location2_id);

    cleanup_resident(&client, community_id, resident_id).await;
    cleanup_location(&client, community_id, location1_id).await;
    cleanup_location(&client, community_id, location2_id).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_update_resident_move_to_different_community() {
    let client = Client::new();
    let community1_id = create_test_community(&client, "Community 1").await;
    let community2_id = create_test_community(&client, "Community 2").await;
    let location1_id =
        create_test_location(&client, community1_id, "Location in Community 1").await;
    let location2_id =
        create_test_location(&client, community2_id, "Location in Community 2").await;
    let resident_id =
        create_test_resident(&client, community1_id, location1_id, "Test Resident").await;

    // Try to move resident to a location in a different community (should fail)
    let response = client
        .put(format!(
            "{}/v1/communities/{}/residents/{}",
            BASE_URL, community1_id, resident_id
        ))
        .json(&json!({"first_name": "Test", "last_name": "Resident", "location_id": location2_id}))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let error: ErrorResponse = response.json().await.expect("Failed to parse error");
    assert!(!error.error.is_empty());

    cleanup_resident(&client, community1_id, resident_id).await;
    cleanup_location(&client, community1_id, location1_id).await;
    cleanup_location(&client, community2_id, location2_id).await;
    cleanup_community(&client, community1_id).await;
    cleanup_community(&client, community2_id).await;
}

#[tokio::test]
async fn test_update_resident_not_found() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Test Community").await;
    let location_id = create_test_location(&client, community_id, "Test Location").await;
    let fake_resident_id = Uuid::new_v4();

    let response = client
        .put(format!(
            "{}/v1/communities/{}/residents/{}",
            BASE_URL, community_id, fake_resident_id
        ))
        .json(&json!({"first_name": "New", "last_name": "Name", "location_id": location_id}))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_location(&client, community_id, location_id).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_update_resident_missing_location_id() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Partial Update Community").await;
    let location_id = create_test_location(&client, community_id, "Test Location").await;
    let resident_id =
        create_test_resident(&client, community_id, location_id, "Original Name").await;

    let new_name = "Just New Name";
    let (new_first_name, new_last_name) = split_name(new_name);
    let response = client
        .put(format!(
            "{}/v1/communities/{}/residents/{}",
            BASE_URL, community_id, resident_id
        ))
        .json(&json!({"first_name": new_first_name, "last_name": new_last_name}))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup_resident(&client, community_id, resident_id).await;
    cleanup_location(&client, community_id, location_id).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_update_resident_missing_first_name() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Partial Update Community Loc").await;
    let location1_id = create_test_location(&client, community_id, "Location 1").await;
    let location2_id = create_test_location(&client, community_id, "Location 2").await;
    let resident_name = "Static Name";
    let resident_id =
        create_test_resident(&client, community_id, location1_id, resident_name).await;

    let response = client
        .put(format!(
            "{}/v1/communities/{}/residents/{}",
            BASE_URL, community_id, resident_id
        ))
        .json(&json!({"last_name": "Name", "location_id": location2_id}))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup_resident(&client, community_id, resident_id).await;
    cleanup_location(&client, community_id, location1_id).await;
    cleanup_location(&client, community_id, location2_id).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_create_resident_last_name_too_long() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Long Name Community").await;
    let location_id = create_test_location(&client, community_id, "Test Location").await;

    let long_name = "a".repeat(1001);
    let response = client
        .post(format!(
            "{}/v1/communities/{}/residents",
            BASE_URL, community_id
        ))
        .json(&json!({"first_name": "Test", "last_name": long_name, "location_id": location_id}))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let error: ErrorResponse = response.json().await.expect("Failed to parse error");
    assert!(error.error.contains("exceeds maximum length"));

    cleanup_location(&client, community_id, location_id).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_update_resident_empty_last_name() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Empty Name Update").await;
    let location_id = create_test_location(&client, community_id, "Test Location").await;
    let resident_id =
        create_test_resident(&client, community_id, location_id, "Original Name").await;

    let response = client
        .put(format!(
            "{}/v1/communities/{}/residents/{}",
            BASE_URL, community_id, resident_id
        ))
        .json(&json!({"first_name": "Test", "last_name": "", "location_id": location_id}))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup_resident(&client, community_id, resident_id).await;
    cleanup_location(&client, community_id, location_id).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_delete_resident_success() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Delete Test Community").await;
    let location_id = create_test_location(&client, community_id, "Test Location").await;
    let resident_id = create_test_resident(&client, community_id, location_id, "To Delete").await;

    let response = client
        .delete(format!(
            "{}/v1/communities/{}/residents/{}",
            BASE_URL, community_id, resident_id
        ))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let get_response = client
        .get(format!(
            "{}/v1/communities/{}/residents/{}",
            BASE_URL, community_id, resident_id
        ))
        .send()
        .await
        .expect("Failed to get resident");

    assert_eq!(get_response.status(), StatusCode::NOT_FOUND);

    cleanup_location(&client, community_id, location_id).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_delete_resident_not_found() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Test Community").await;
    let fake_resident_id = Uuid::new_v4();

    let response = client
        .delete(format!(
            "{}/v1/communities/{}/residents/{}",
            BASE_URL, community_id, fake_resident_id
        ))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_delete_resident_wrong_community() {
    let client = Client::new();
    let community1_id = create_test_community(&client, "Community 1").await;
    let community2_id = create_test_community(&client, "Community 2").await;
    let location_id = create_test_location(&client, community1_id, "Test Location").await;
    let resident_id =
        create_test_resident(&client, community1_id, location_id, "Test Resident").await;

    let response = client
        .delete(format!(
            "{}/v1/communities/{}/residents/{}",
            BASE_URL, community2_id, resident_id
        ))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let get_response = client
        .get(format!(
            "{}/v1/communities/{}/residents/{}",
            BASE_URL, community1_id, resident_id
        ))
        .send()
        .await
        .expect("Failed to get resident");

    assert_eq!(get_response.status(), StatusCode::OK);

    cleanup_resident(&client, community1_id, resident_id).await;
    cleanup_location(&client, community1_id, location_id).await;
    cleanup_community(&client, community1_id).await;
    cleanup_community(&client, community2_id).await;
}

#[tokio::test]
async fn test_list_residents_with_valid_location() {
    let client = Client::new();
    let community_id = create_test_community(&client, "List Filter Community").await;
    let location1_id = create_test_location(&client, community_id, "Location 1").await;
    let location2_id = create_test_location(&client, community_id, "Location 2").await;

    let id1 = create_test_resident(&client, community_id, location1_id, "Resident 1").await;
    let id2 = create_test_resident(&client, community_id, location2_id, "Resident 2").await;
    let id3 = create_test_resident(&client, community_id, location1_id, "Resident 3").await;

    let response = client
        .get(format!(
            "{}/v1/communities/{}/residents?location_id={}",
            BASE_URL, community_id, location1_id
        ))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    let residents: Vec<Resident> = response.json().await.expect("Failed to parse response");
    assert_eq!(residents.len(), 2);

    for resident in &residents {
        assert_eq!(resident.location_id, location1_id);
    }

    cleanup_resident(&client, community_id, id1).await;
    cleanup_resident(&client, community_id, id2).await;
    cleanup_resident(&client, community_id, id3).await;
    cleanup_location(&client, community_id, location1_id).await;
    cleanup_location(&client, community_id, location2_id).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_list_residents_with_location_no_residents() {
    let client = Client::new();
    let community_id = create_test_community(&client, "List Empty Loc Community").await;
    let location1_id = create_test_location(&client, community_id, "Location 1").await;
    let location2_id = create_test_location(&client, community_id, "Location 2").await;

    // Add resident to location 2 only
    let id2 = create_test_resident(&client, community_id, location2_id, "Resident 2").await;

    let response = client
        .get(format!(
            "{}/v1/communities/{}/residents?location_id={}",
            BASE_URL, community_id, location1_id
        ))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    let residents: Vec<Resident> = response.json().await.expect("Failed to parse response");
    assert_eq!(residents.len(), 0);

    cleanup_resident(&client, community_id, id2).await;
    cleanup_location(&client, community_id, location1_id).await;
    cleanup_location(&client, community_id, location2_id).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_list_residents_with_invalid_location() {
    let client = Client::new();
    let community_id = create_test_community(&client, "List Invalid Loc Community").await;
    let fake_id = Uuid::new_v4();

    let response = client
        .get(format!(
            "{}/v1/communities/{}/residents?location_id={}",
            BASE_URL, community_id, fake_id
        ))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_list_residents_with_location_different_community() {
    let client = Client::new();
    let community1_id = create_test_community(&client, "Com 1").await;
    let community2_id = create_test_community(&client, "Com 2").await;
    let location1_id = create_test_location(&client, community1_id, "Loc 1").await;

    let response = client
        .get(format!(
            "{}/v1/communities/{}/residents?location_id={}",
            BASE_URL, community2_id, location1_id
        ))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup_location(&client, community1_id, location1_id).await;
    cleanup_community(&client, community1_id).await;
    cleanup_community(&client, community2_id).await;
}

#[tokio::test]
async fn test_update_resident_last_name_too_long() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Test Community").await;
    let location_id = create_test_location(&client, community_id, "Test Location").await;
    let resident_id =
        create_test_resident(&client, community_id, location_id, "Test Resident").await;

    let long_name = "a".repeat(1001);
    let response = client
        .put(format!(
            "{}/v1/communities/{}/residents/{}",
            BASE_URL, community_id, resident_id
        ))
        .json(&json!({"first_name": "Test", "last_name": long_name, "location_id": location_id}))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let error: ErrorResponse = response.json().await.expect("Failed to parse error");
    assert!(error.error.contains("maximum length"));

    cleanup_resident(&client, community_id, resident_id).await;
    cleanup_location(&client, community_id, location_id).await;
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
async fn test_create_resident_with_yardi_reference_id() {
    let client = Client::new();
    let community_id = create_yardi_community(&client, "Yardi Resident Test").await;
    let location_id = create_test_location(&client, community_id, "Test Location").await;

    let response = client
        .post(format!(
            "{}/v1/communities/{}/residents",
            BASE_URL, community_id
        ))
        .json(&json!({
            "first_name": "Yardi",
            "last_name": "Resident",
            "location_id": location_id,
            "yardi_reference_id": "RES-001"
        }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::CREATED);

    let resident: Resident = response.json().await.expect("Failed to parse response");
    assert_eq!(resident.yardi_reference_id, Some("RES-001".to_string()));
    assert_eq!(resident.community_id, community_id);

    cleanup_resident(&client, community_id, resident.id).await;
    cleanup_location(&client, community_id, location_id).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_create_resident_yardi_reference_id_without_integration_fails() {
    let client = Client::new();
    let community_id = create_test_community(&client, "No Yardi Community").await;
    let location_id = create_test_location(&client, community_id, "Test Location").await;

    let response = client
        .post(format!(
            "{}/v1/communities/{}/residents",
            BASE_URL, community_id
        ))
        .json(&json!({
            "first_name": "Test",
            "last_name": "Resident",
            "location_id": location_id,
            "yardi_reference_id": "RES-001"
        }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::CONFLICT);

    let error: ErrorResponse = response.json().await.expect("Failed to parse error");
    assert!(error.error.contains("Yardi integration"));

    cleanup_location(&client, community_id, location_id).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_resident_yardi_reference_id_unique_in_community() {
    let client = Client::new();
    let community_id = create_yardi_community(&client, "Yardi Unique Resident Test").await;
    let location_id = create_test_location(&client, community_id, "Test Location").await;

    // Create first resident with yardi_reference_id
    let response = client
        .post(format!(
            "{}/v1/communities/{}/residents",
            BASE_URL, community_id
        ))
        .json(&json!({
            "first_name": "Resident",
            "last_name": "1",
            "location_id": location_id,
            "yardi_reference_id": "RES-001"
        }))
        .send()
        .await
        .expect("Failed to create resident");

    let resident1: Resident = response.json().await.expect("Failed to parse resident");

    // Try to create second resident with same yardi_reference_id
    let response = client
        .post(format!(
            "{}/v1/communities/{}/residents",
            BASE_URL, community_id
        ))
        .json(&json!({
            "first_name": "Resident",
            "last_name": "2",
            "location_id": location_id,
            "yardi_reference_id": "RES-001"
        }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::CONFLICT);

    let error: ErrorResponse = response.json().await.expect("Failed to parse error");
    assert!(error.error.contains("already exists"));

    cleanup_resident(&client, community_id, resident1.id).await;
    cleanup_location(&client, community_id, location_id).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_cannot_unset_yardi_integration_with_resident_reference() {
    let client = Client::new();
    let community_id = create_yardi_community(&client, "Yardi Unset Resident Test").await;
    let location_id = create_test_location(&client, community_id, "Test Location").await;

    // Create resident with yardi_reference_id
    let response = client
        .post(format!(
            "{}/v1/communities/{}/residents",
            BASE_URL, community_id
        ))
        .json(&json!({
            "first_name": "Yardi",
            "last_name": "Resident",
            "location_id": location_id,
            "yardi_reference_id": "RES-001"
        }))
        .send()
        .await
        .expect("Failed to create resident");

    let resident: Resident = response.json().await.expect("Failed to parse resident");

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

    cleanup_resident(&client, community_id, resident.id).await;
    cleanup_location(&client, community_id, location_id).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_resident_community_id_in_response() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Community ID Test").await;
    let location_id = create_test_location(&client, community_id, "Test Location").await;

    let response = client
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
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::CREATED);

    let resident: Resident = response.json().await.expect("Failed to parse response");
    assert_eq!(resident.community_id, community_id);

    cleanup_resident(&client, community_id, resident.id).await;
    cleanup_location(&client, community_id, location_id).await;
    cleanup_community(&client, community_id).await;
}

// --- Resident Photo Tests ---

#[tokio::test]
async fn test_resident_photo_lifecycle_and_metadata() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Photo Lifecycle Community").await;
    let location_id = create_test_location(&client, community_id, "Photo Location").await;
    let resident_id =
        create_test_resident(&client, community_id, location_id, "Photo Resident").await;

    let photo_bytes = vec![
        0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 1, 2, 3, 4, 5, 6,
    ];
    let put_response = client
        .put(resident_photo_url(community_id, resident_id))
        .header(header::CONTENT_TYPE, "image/png")
        .body(photo_bytes.clone())
        .send()
        .await
        .expect("Failed to upload resident photo");
    assert_eq!(put_response.status(), StatusCode::OK);
    let updated_resident: Resident = put_response
        .json()
        .await
        .expect("Failed to parse resident after photo upload");
    let uploaded_photo = updated_resident
        .photo
        .expect("photo metadata should be present");
    assert_eq!(uploaded_photo.content_type, "image/png");
    assert_eq!(uploaded_photo.size_bytes as usize, photo_bytes.len());
    assert!(uploaded_photo.etag.starts_with("sha256:"));
    assert!(!uploaded_photo.updated_at.is_empty());

    let get_resident_response = client
        .get(format!(
            "{}/v1/communities/{}/residents/{}",
            BASE_URL, community_id, resident_id
        ))
        .send()
        .await
        .expect("Failed to get resident");
    assert_eq!(get_resident_response.status(), StatusCode::OK);
    let resident: Resident = get_resident_response
        .json()
        .await
        .expect("Failed to parse resident");
    assert_eq!(resident.photo, Some(uploaded_photo.clone()));

    let list_response = client
        .get(format!(
            "{}/v1/communities/{}/residents",
            BASE_URL, community_id
        ))
        .send()
        .await
        .expect("Failed to list residents");
    assert_eq!(list_response.status(), StatusCode::OK);
    let residents: Vec<Resident> = list_response
        .json()
        .await
        .expect("Failed to parse resident list");
    let listed = residents
        .iter()
        .find(|r| r.id == resident_id)
        .expect("Uploaded resident should be in list");
    assert_eq!(listed.photo, Some(uploaded_photo.clone()));

    let get_photo_response = client
        .get(resident_photo_url(community_id, resident_id))
        .send()
        .await
        .expect("Failed to get resident photo");
    assert_eq!(get_photo_response.status(), StatusCode::OK);
    assert_eq!(
        get_photo_response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some("image/png")
    );
    let expected_etag_header = format!("\"{}\"", uploaded_photo.etag);
    assert_eq!(
        get_photo_response
            .headers()
            .get(header::ETAG)
            .and_then(|v| v.to_str().ok()),
        Some(expected_etag_header.as_str())
    );
    assert!(
        get_photo_response
            .headers()
            .get(header::LAST_MODIFIED)
            .is_some()
    );
    let downloaded = get_photo_response
        .bytes()
        .await
        .expect("Failed to read photo bytes");
    assert_eq!(downloaded.as_ref(), photo_bytes.as_slice());

    let delete_response = client
        .delete(resident_photo_url(community_id, resident_id))
        .send()
        .await
        .expect("Failed to delete resident photo");
    assert_eq!(delete_response.status(), StatusCode::NO_CONTENT);

    let resident_after_delete: Resident = client
        .get(format!(
            "{}/v1/communities/{}/residents/{}",
            BASE_URL, community_id, resident_id
        ))
        .send()
        .await
        .expect("Failed to get resident after photo delete")
        .json()
        .await
        .expect("Failed to parse resident after photo delete");
    assert!(resident_after_delete.photo.is_none());

    cleanup_resident(&client, community_id, resident_id).await;
    cleanup_location(&client, community_id, location_id).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_get_photo_if_none_match_returns_not_modified() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Photo ETag Community").await;
    let location_id = create_test_location(&client, community_id, "Photo Location").await;
    let resident_id =
        create_test_resident(&client, community_id, location_id, "Photo Resident").await;

    let put_response = client
        .put(resident_photo_url(community_id, resident_id))
        .header(header::CONTENT_TYPE, "image/jpeg")
        .body(vec![0xff, 0xd8, 0xff, 0xdb, 1, 2, 3, 4])
        .send()
        .await
        .expect("Failed to upload resident photo");
    assert_eq!(put_response.status(), StatusCode::OK);
    let resident: Resident = put_response.json().await.expect("Failed to parse resident");
    let etag = resident.photo.expect("photo should be present").etag;

    let response = client
        .get(resident_photo_url(community_id, resident_id))
        .header(header::IF_NONE_MATCH, format!("\"{}\"", etag))
        .send()
        .await
        .expect("Failed to get photo with If-None-Match");
    assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
    let body = response
        .bytes()
        .await
        .expect("Failed to read response body");
    assert!(body.is_empty());

    cleanup_resident(&client, community_id, resident_id).await;
    cleanup_location(&client, community_id, location_id).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_put_photo_missing_content_type() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Photo Missing Content-Type").await;
    let location_id = create_test_location(&client, community_id, "Photo Location").await;
    let resident_id =
        create_test_resident(&client, community_id, location_id, "Photo Resident").await;

    let response = client
        .put(resident_photo_url(community_id, resident_id))
        .body(vec![1, 2, 3])
        .send()
        .await
        .expect("Failed to send request");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let error: ErrorResponse = response.json().await.expect("Failed to parse error");
    assert_eq!(error.reason, "resident_photo_content_type_required");

    cleanup_resident(&client, community_id, resident_id).await;
    cleanup_location(&client, community_id, location_id).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_put_photo_invalid_content_type() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Photo Invalid Content-Type").await;
    let location_id = create_test_location(&client, community_id, "Photo Location").await;
    let resident_id =
        create_test_resident(&client, community_id, location_id, "Photo Resident").await;

    let response = client
        .put(resident_photo_url(community_id, resident_id))
        .header(header::CONTENT_TYPE, "text/plain")
        .body(vec![1, 2, 3])
        .send()
        .await
        .expect("Failed to send request");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let error: ErrorResponse = response.json().await.expect("Failed to parse error");
    assert_eq!(error.reason, "resident_photo_content_type_invalid");

    cleanup_resident(&client, community_id, resident_id).await;
    cleanup_location(&client, community_id, location_id).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_put_photo_accepts_content_type_parameters() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Photo Content-Type Params").await;
    let location_id = create_test_location(&client, community_id, "Photo Location").await;
    let resident_id =
        create_test_resident(&client, community_id, location_id, "Photo Resident").await;

    let response = client
        .put(resident_photo_url(community_id, resident_id))
        .header(header::CONTENT_TYPE, "image/jpeg; charset=binary")
        .body(vec![0xff, 0xd8, 0xff, 0xd9])
        .send()
        .await
        .expect("Failed to upload resident photo");
    assert_eq!(response.status(), StatusCode::OK);
    let resident: Resident = response.json().await.expect("Failed to parse resident");
    assert_eq!(
        resident
            .photo
            .expect("photo should be present")
            .content_type
            .as_str(),
        "image/jpeg"
    );

    cleanup_resident(&client, community_id, resident_id).await;
    cleanup_location(&client, community_id, location_id).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_put_photo_empty_body() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Photo Empty Body").await;
    let location_id = create_test_location(&client, community_id, "Photo Location").await;
    let resident_id =
        create_test_resident(&client, community_id, location_id, "Photo Resident").await;

    let response = client
        .put(resident_photo_url(community_id, resident_id))
        .header(header::CONTENT_TYPE, "image/png")
        .body(Vec::<u8>::new())
        .send()
        .await
        .expect("Failed to send request");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let error: ErrorResponse = response.json().await.expect("Failed to parse error");
    assert_eq!(error.reason, "resident_photo_empty");

    cleanup_resident(&client, community_id, resident_id).await;
    cleanup_location(&client, community_id, location_id).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_put_photo_too_large() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Photo Too Large").await;
    let location_id = create_test_location(&client, community_id, "Photo Location").await;
    let resident_id =
        create_test_resident(&client, community_id, location_id, "Photo Resident").await;

    let response = client
        .put(resident_photo_url(community_id, resident_id))
        .header(header::CONTENT_TYPE, "image/webp")
        .body(vec![7; 2 * 1024 * 1024 + 1])
        .send()
        .await
        .expect("Failed to send request");
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let error: ErrorResponse = response.json().await.expect("Failed to parse error");
    assert_eq!(error.reason, "resident_photo_too_large");

    cleanup_resident(&client, community_id, resident_id).await;
    cleanup_location(&client, community_id, location_id).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_get_and_delete_photo_not_found_without_upload() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Photo Missing").await;
    let location_id = create_test_location(&client, community_id, "Photo Location").await;
    let resident_id =
        create_test_resident(&client, community_id, location_id, "Photo Resident").await;

    let get_response = client
        .get(resident_photo_url(community_id, resident_id))
        .send()
        .await
        .expect("Failed to get photo");
    assert_eq!(get_response.status(), StatusCode::NOT_FOUND);
    let get_error: ErrorResponse = get_response.json().await.expect("Failed to parse error");
    assert_eq!(get_error.reason, "resident_photo_not_found");

    let delete_response = client
        .delete(resident_photo_url(community_id, resident_id))
        .send()
        .await
        .expect("Failed to delete photo");
    assert_eq!(delete_response.status(), StatusCode::NOT_FOUND);
    let delete_error: ErrorResponse = delete_response.json().await.expect("Failed to parse error");
    assert_eq!(delete_error.reason, "resident_photo_not_found");

    cleanup_resident(&client, community_id, resident_id).await;
    cleanup_location(&client, community_id, location_id).await;
    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_photo_endpoints_resident_not_found() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Photo Missing Resident").await;
    let fake_resident_id = Uuid::new_v4();

    let put_response = client
        .put(resident_photo_url(community_id, fake_resident_id))
        .header(header::CONTENT_TYPE, "image/png")
        .body(vec![1, 2, 3])
        .send()
        .await
        .expect("Failed to upload photo");
    assert_eq!(put_response.status(), StatusCode::NOT_FOUND);
    let put_error: ErrorResponse = put_response.json().await.expect("Failed to parse error");
    assert_eq!(put_error.reason, "resident_not_found");

    let get_response = client
        .get(resident_photo_url(community_id, fake_resident_id))
        .send()
        .await
        .expect("Failed to get photo");
    assert_eq!(get_response.status(), StatusCode::NOT_FOUND);
    let get_error: ErrorResponse = get_response.json().await.expect("Failed to parse error");
    assert_eq!(get_error.reason, "resident_not_found");

    let delete_response = client
        .delete(resident_photo_url(community_id, fake_resident_id))
        .send()
        .await
        .expect("Failed to delete photo");
    assert_eq!(delete_response.status(), StatusCode::NOT_FOUND);
    let delete_error: ErrorResponse = delete_response.json().await.expect("Failed to parse error");
    assert_eq!(delete_error.reason, "resident_not_found");

    cleanup_community(&client, community_id).await;
}

#[tokio::test]
async fn test_photo_endpoints_wrong_community() {
    let client = Client::new();
    let community1_id = create_test_community(&client, "Photo Community One").await;
    let community2_id = create_test_community(&client, "Photo Community Two").await;
    let location_id = create_test_location(&client, community1_id, "Photo Location").await;
    let resident_id =
        create_test_resident(&client, community1_id, location_id, "Photo Resident").await;

    let put_response = client
        .put(resident_photo_url(community2_id, resident_id))
        .header(header::CONTENT_TYPE, "image/png")
        .body(vec![1, 2, 3])
        .send()
        .await
        .expect("Failed to upload photo");
    assert_eq!(put_response.status(), StatusCode::NOT_FOUND);
    let put_error: ErrorResponse = put_response.json().await.expect("Failed to parse error");
    assert_eq!(put_error.reason, "resident_not_found");

    let get_response = client
        .get(resident_photo_url(community2_id, resident_id))
        .send()
        .await
        .expect("Failed to get photo");
    assert_eq!(get_response.status(), StatusCode::NOT_FOUND);
    let get_error: ErrorResponse = get_response.json().await.expect("Failed to parse error");
    assert_eq!(get_error.reason, "resident_not_found");

    let delete_response = client
        .delete(resident_photo_url(community2_id, resident_id))
        .send()
        .await
        .expect("Failed to delete photo");
    assert_eq!(delete_response.status(), StatusCode::NOT_FOUND);
    let delete_error: ErrorResponse = delete_response.json().await.expect("Failed to parse error");
    assert_eq!(delete_error.reason, "resident_not_found");

    cleanup_resident(&client, community1_id, resident_id).await;
    cleanup_location(&client, community1_id, location_id).await;
    cleanup_community(&client, community1_id).await;
    cleanup_community(&client, community2_id).await;
}

#[tokio::test]
async fn test_resident_delete_cascades_photo() {
    let client = Client::new();
    let community_id = create_test_community(&client, "Photo Cascade Community").await;
    let location_id = create_test_location(&client, community_id, "Photo Location").await;
    let resident_id =
        create_test_resident(&client, community_id, location_id, "Photo Resident").await;

    let put_response = client
        .put(resident_photo_url(community_id, resident_id))
        .header(header::CONTENT_TYPE, "image/png")
        .body(vec![1, 2, 3, 4])
        .send()
        .await
        .expect("Failed to upload photo");
    assert_eq!(put_response.status(), StatusCode::OK);

    let delete_resident_response = client
        .delete(format!(
            "{}/v1/communities/{}/residents/{}",
            BASE_URL, community_id, resident_id
        ))
        .send()
        .await
        .expect("Failed to delete resident");
    assert_eq!(delete_resident_response.status(), StatusCode::NO_CONTENT);

    let get_photo_response = client
        .get(resident_photo_url(community_id, resident_id))
        .send()
        .await
        .expect("Failed to get photo");
    assert_eq!(get_photo_response.status(), StatusCode::NOT_FOUND);
    let error: ErrorResponse = get_photo_response
        .json()
        .await
        .expect("Failed to parse error");
    assert_eq!(error.reason, "resident_not_found");

    cleanup_location(&client, community_id, location_id).await;
    cleanup_community(&client, community_id).await;
}
