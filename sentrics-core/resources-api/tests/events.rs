use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

const BASE_URL: &str = "http://127.0.0.1:9000/lambda-url/resources-api";
const QUEUE_URL: &str = "http://127.0.0.1:4566/000000000000/resources-events-test";

#[derive(Debug, Serialize, Deserialize)]
struct Community {
    id: Uuid,
    name: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Location {
    id: Uuid,
    community_id: Uuid,
    name: String,
    location_type: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Resident {
    id: Uuid,
    location_id: Uuid,
    first_name: String,
    last_name: String,
}

#[derive(Debug, Deserialize)]
struct SnsNotification {
    #[serde(rename = "Message")]
    message: String,
}

#[derive(Debug, Deserialize)]
struct EventMessage {
    event_id: Uuid,
    resource_type: String,
    event_type: String,
    timestamp: String,
    requester: serde_json::Value,
    #[serde(default)]
    before: Option<serde_json::Value>,
    #[serde(default)]
    after: Option<serde_json::Value>,
}

async fn purge_queue(client: &Client) {
    let _ = client
        .get(format!("{}?Action=PurgeQueue", QUEUE_URL))
        .send()
        .await;
}

async fn receive_event(client: &Client) -> Option<EventMessage> {
    receive_event_with_max_polls(client, 30).await
}

async fn receive_event_with_max_polls(client: &Client, max_polls: usize) -> Option<EventMessage> {
    for _ in 0..max_polls {
        let resp = match client
            .get(format!(
                "{}?Action=ReceiveMessage&WaitTimeSeconds=2",
                QUEUE_URL
            ))
            .send()
            .await
        {
            Ok(r) => r,
            Err(_) => continue,
        };

        let text = match resp.text().await {
            Ok(t) => t,
            Err(_) => continue,
        };

        let body_start = match text.find("<Body>") {
            Some(pos) => pos,
            None => continue,
        };
        let body_end = match text.find("</Body>") {
            Some(pos) => pos,
            None => continue,
        };
        let body_xml = &text[body_start + 6..body_end];

        let body_decoded = body_xml
            .replace("&quot;", "\"")
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("\\&quot;", "\"");

        if let (Some(receipt_start), Some(receipt_end)) =
            (text.find("<ReceiptHandle>"), text.find("</ReceiptHandle>"))
        {
            let receipt = &text[receipt_start + 15..receipt_end];
            let _ = client
                .get(format!(
                    "{}?Action=DeleteMessage&ReceiptHandle={}",
                    QUEUE_URL, receipt
                ))
                .send()
                .await;
        }

        let sns_notification: SnsNotification = match serde_json::from_str(&body_decoded) {
            Ok(n) => n,
            Err(_) => continue,
        };

        match serde_json::from_str(&sns_notification.message) {
            Ok(event) => return Some(event),
            Err(_) => continue,
        }
    }
    None
}

#[tokio::test]
async fn test_event_publishing() {
    let client = Client::new();
    purge_queue(&client).await;

    // Test community create event
    let community_name = format!("Test Community {}", Uuid::new_v4());
    let response = client
        .post(format!("{}/v1/communities", BASE_URL))
        .json(&json!({"name": community_name}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let community: Community = response.json().await.unwrap();

    let event = receive_event(&client)
        .await
        .expect("No community create event");
    assert_eq!(event.resource_type, "community");
    assert_eq!(event.event_type, "create");
    assert_event_id(&event.event_id);
    assert!(event.before.is_none());
    assert_eq!(event.after.as_ref().unwrap()["name"], json!(community_name));

    // Verify timestamp is a valid RFC3339 timestamp and reasonably recent
    assert!(
        chrono::DateTime::parse_from_rfc3339(&event.timestamp).is_ok(),
        "Timestamp should be valid RFC3339 format"
    );
    let event_time = chrono::DateTime::parse_from_rfc3339(&event.timestamp).unwrap();
    let now = chrono::Utc::now();
    let diff = now.signed_duration_since(event_time);
    assert!(
        diff.num_seconds() >= 0 && diff.num_seconds() < 30,
        "Event timestamp should be within last 30 seconds"
    );

    // Verify requester info is present (local-dev for local testing)
    assert_requester(&event.requester);

    // Test community update event
    let updated_community_name = format!("Updated Community {}", Uuid::new_v4());
    let response = client
        .put(format!("{}/v1/communities/{}", BASE_URL, community.id))
        .json(&json!({"name": updated_community_name}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let event = receive_event(&client)
        .await
        .expect("No community update event");
    assert_eq!(event.resource_type, "community");
    assert_eq!(event.event_type, "update");
    assert_event_id(&event.event_id);
    assert_eq!(
        event.before.as_ref().unwrap()["name"],
        json!(community_name)
    );
    assert_eq!(
        event.after.as_ref().unwrap()["name"],
        json!(updated_community_name)
    );

    // Test location create event
    let location_name = format!("Test Location {}", Uuid::new_v4());
    let response = client
        .post(format!(
            "{}/v1/communities/{}/locations",
            BASE_URL, community.id
        ))
        .json(&json!({"name": location_name, "location_type": "apartment"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let location: Location = response.json().await.unwrap();

    let event = receive_event(&client)
        .await
        .expect("No location create event");
    assert_eq!(event.resource_type, "location");
    assert_eq!(event.event_type, "create");
    assert_event_id(&event.event_id);
    assert_eq!(event.after.as_ref().unwrap()["name"], json!(location_name));
    assert_eq!(
        event.after.as_ref().unwrap()["location_type"],
        json!("apartment")
    );

    // Test location update event
    let updated_location_name = format!("Updated Location {}", Uuid::new_v4());
    let response = client
        .put(format!(
            "{}/v1/communities/{}/locations/{}",
            BASE_URL, community.id, location.id
        ))
        .json(&json!({"name": updated_location_name, "location_type": "apartment"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let event = receive_event(&client)
        .await
        .expect("No location update event");
    assert_eq!(event.resource_type, "location");
    assert_eq!(event.event_type, "update");
    assert_event_id(&event.event_id);
    assert_eq!(event.before.as_ref().unwrap()["name"], json!(location_name));
    assert_eq!(
        event.before.as_ref().unwrap()["location_type"],
        json!("apartment")
    );
    assert_eq!(
        event.after.as_ref().unwrap()["name"],
        json!(updated_location_name)
    );
    assert_eq!(
        event.after.as_ref().unwrap()["location_type"],
        json!("apartment")
    );

    // Test resident create event
    let resident_last_name = format!("Resident {}", Uuid::new_v4());
    let response = client
        .post(format!(
            "{}/v1/communities/{}/residents",
            BASE_URL, community.id
        ))
        .json(&json!({
            "first_name": "Test",
            "last_name": resident_last_name,
            "location_id": location.id
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let resident: Resident = response.json().await.unwrap();

    let event = receive_event(&client)
        .await
        .expect("No resident create event");
    assert_eq!(event.resource_type, "resident");
    assert_eq!(event.event_type, "create");
    assert_event_id(&event.event_id);
    assert_eq!(event.after.as_ref().unwrap()["first_name"], json!("Test"));
    assert_eq!(
        event.after.as_ref().unwrap()["last_name"],
        json!(resident_last_name)
    );

    // Test resident update event
    let updated_resident_last_name = format!("Resident {}", Uuid::new_v4());
    let response = client
        .put(format!(
            "{}/v1/communities/{}/residents/{}",
            BASE_URL, community.id, resident.id
        ))
        .json(&json!({
            "first_name": "Updated",
            "last_name": updated_resident_last_name,
            "location_id": location.id
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let event = receive_event(&client)
        .await
        .expect("No resident update event");
    assert_eq!(event.resource_type, "resident");
    assert_eq!(event.event_type, "update");
    assert_event_id(&event.event_id);
    assert_eq!(event.before.as_ref().unwrap()["first_name"], json!("Test"));
    assert_eq!(
        event.before.as_ref().unwrap()["last_name"],
        json!(resident_last_name)
    );
    assert_eq!(
        event.after.as_ref().unwrap()["first_name"],
        json!("Updated")
    );
    assert_eq!(
        event.after.as_ref().unwrap()["last_name"],
        json!(updated_resident_last_name)
    );

    // Test resident photo update event (metadata only)
    let photo_bytes = vec![0x89, b'P', b'N', b'G', 1, 2, 3, 4, 5, 6];
    let response = client
        .put(format!(
            "{}/v1/communities/{}/residents/{}/photo",
            BASE_URL, community.id, resident.id
        ))
        .header("Content-Type", "image/png")
        .body(photo_bytes.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let event = receive_event(&client)
        .await
        .expect("No resident photo update event");
    assert_eq!(event.resource_type, "resident");
    assert_eq!(event.event_type, "update");
    let before_photo = &event.before.as_ref().unwrap()["photo"];
    let after_photo = &event.after.as_ref().unwrap()["photo"];
    assert!(before_photo.is_null());
    assert!(
        after_photo["etag"]
            .as_str()
            .unwrap_or_default()
            .starts_with("sha256:")
    );
    assert_eq!(after_photo["content_type"], json!("image/png"));
    assert_eq!(after_photo["size_bytes"], json!(photo_bytes.len()));
    assert!(after_photo.get("updated_at").is_some());
    assert!(after_photo.get("image_data").is_none());

    // Re-upload exact same photo: no event expected.
    let response = client
        .put(format!(
            "{}/v1/communities/{}/residents/{}/photo",
            BASE_URL, community.id, resident.id
        ))
        .header("Content-Type", "image/png")
        .body(photo_bytes)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let no_event = receive_event_with_max_polls(&client, 2).await;
    assert!(
        no_event.is_none(),
        "Expected no event for unchanged photo upload"
    );

    // Test resident photo delete event (metadata only)
    let response = client
        .delete(format!(
            "{}/v1/communities/{}/residents/{}/photo",
            BASE_URL, community.id, resident.id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let event = receive_event(&client)
        .await
        .expect("No resident photo delete event");
    assert_eq!(event.resource_type, "resident");
    assert_eq!(event.event_type, "update");
    assert!(event.before.as_ref().unwrap()["photo"].is_object());
    assert!(event.after.as_ref().unwrap()["photo"].is_null());

    // Test resident delete event
    let response = client
        .delete(format!(
            "{}/v1/communities/{}/residents/{}",
            BASE_URL, community.id, resident.id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let event = receive_event(&client)
        .await
        .expect("No resident delete event");
    assert_eq!(event.resource_type, "resident");
    assert_eq!(event.event_type, "delete");
    assert_event_id(&event.event_id);
    assert_eq!(
        event.before.as_ref().unwrap()["first_name"],
        json!("Updated")
    );
    assert_eq!(
        event.before.as_ref().unwrap()["last_name"],
        json!(updated_resident_last_name)
    );
    assert!(event.after.is_none());

    // Test location delete event
    let response = client
        .delete(format!(
            "{}/v1/communities/{}/locations/{}",
            BASE_URL, community.id, location.id
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let event = receive_event(&client)
        .await
        .expect("No location delete event");
    assert_eq!(event.resource_type, "location");
    assert_eq!(event.event_type, "delete");
    assert_event_id(&event.event_id);
    assert_eq!(
        event.before.as_ref().unwrap()["name"],
        json!(updated_location_name)
    );
    assert!(event.after.is_none());

    // Test community delete event
    let response = client
        .delete(format!("{}/v1/communities/{}", BASE_URL, community.id))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let event = receive_event(&client)
        .await
        .expect("No community delete event");
    assert_eq!(event.resource_type, "community");
    assert_eq!(event.event_type, "delete");
    assert_event_id(&event.event_id);
    assert_eq!(
        event.before.as_ref().unwrap()["name"],
        json!(updated_community_name)
    );
    assert!(event.after.is_none());
}

fn assert_requester(requester: &serde_json::Value) {
    let requester_type = requester
        .get("type")
        .and_then(|value| value.as_str())
        .expect("Requester must include a type");
    match requester_type {
        "local_dev" => {}
        "entra_user" => {
            assert_non_empty(requester, "username");
        }
        "iam_assumed_role" => {
            assert_non_empty(requester, "account_id");
            assert_non_empty(requester, "role_name");
            assert_non_empty(requester, "session_name");
        }
        "iam_user" => {
            assert_non_empty(requester, "account_id");
            assert_non_empty(requester, "user_name");
        }
        "iam_federated_user" => {
            assert_non_empty(requester, "account_id");
            assert_non_empty(requester, "user_name");
        }
        "iam_root" => {
            assert_non_empty(requester, "account_id");
        }
        other => panic!("Unsupported requester type: {other}"),
    }
}

fn assert_event_id(event_id: &Uuid) {
    assert_ne!(*event_id, Uuid::nil(), "Event ID must be non-nil");
}

fn assert_non_empty(value: &serde_json::Value, field: &str) {
    let field_value = value
        .get(field)
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    assert!(!field_value.is_empty(), "Requester field {field} is empty");
}
