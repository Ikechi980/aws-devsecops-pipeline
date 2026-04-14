mod common;

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn websocket_connection_requires_client_cert() {
    // Try connecting without client certificate - should fail
    let result = tokio_tungstenite::connect_async(common::WS_URL).await;

    // Connection should fail because nginx requires client cert
    assert!(
        result.is_err(),
        "Connection without client cert should fail"
    );
}

#[tokio::test]
async fn websocket_connection_with_valid_cert_succeeds() {
    let connector = common::load_tls_connector("TestDevice02").await;

    let request = common::WS_URL
        .parse::<tokio_tungstenite::tungstenite::http::Uri>()
        .unwrap();

    let (ws_stream, response) =
        tokio_tungstenite::connect_async_tls_with_config(request, None, false, Some(connector))
            .await
            .expect("Failed to connect");

    assert_eq!(response.status(), 101); // Switching Protocols

    // Clean disconnect
    let (mut write, _read) = ws_stream.split();
    write.close().await.ok();
}

#[tokio::test]
async fn websocket_message_routing() {
    // This test combines:
    // 1. Receiving messages for the correct client
    // 2. Ignoring messages for other clients
    // Uses TestDevice03 to avoid conflicts with other tests

    let connector = common::load_tls_connector("TestDevice03").await;
    let request = common::WS_URL
        .parse::<tokio_tungstenite::tungstenite::http::Uri>()
        .unwrap();

    let (ws_stream, _) =
        tokio_tungstenite::connect_async_tls_with_config(request, None, false, Some(connector))
            .await
            .expect("Failed to connect");

    let (mut write, mut read) = ws_stream.split();

    // Give the connection time to register
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Test 1: Publish a message targeting our client community ID
    common::publish_sns_message(
        "testdevice03",
        "core_change_event",
        vec![(1, serde_json::json!("Hello from SNS!"))],
    )
    .await
    .expect("Failed to publish SNS message");

    // Wait for the message with timeout
    let timeout = tokio::time::timeout(tokio::time::Duration::from_secs(5), read.next()).await;

    match timeout {
        Ok(Some(Ok(Message::Text(text)))) => {
            let payload: serde_json::Value =
                serde_json::from_str(&text).expect("Invalid JSON payload");
            assert_eq!(
                payload
                    .get("message_type")
                    .and_then(serde_json::Value::as_str),
                Some("core_change_event")
            );
            let versions = payload
                .get("versions")
                .and_then(serde_json::Value::as_array)
                .expect("Missing versions");
            assert_eq!(versions.len(), 1);
            assert_eq!(
                versions[0]
                    .get("version")
                    .and_then(serde_json::Value::as_u64),
                Some(1)
            );
            assert_eq!(
                versions[0]
                    .get("payload")
                    .and_then(serde_json::Value::as_str),
                Some("Hello from SNS!")
            );
        }
        Ok(Some(Ok(msg))) => {
            panic!("Unexpected message type: {:?}", msg);
        }
        Ok(Some(Err(e))) => {
            panic!("WebSocket error: {}", e);
        }
        Ok(None) => {
            panic!("WebSocket closed unexpectedly");
        }
        Err(_) => {
            panic!("Timed out waiting for message");
        }
    }

    // Test 2: Publish a message targeting a different client
    common::publish_sns_message(
        "otherdevice",
        "core_change_event",
        vec![(1, serde_json::json!("This should not be received"))],
    )
    .await
    .expect("Failed to publish SNS message");

    // Wait briefly - we should NOT receive this message
    let timeout = tokio::time::timeout(tokio::time::Duration::from_millis(500), read.next()).await;

    // Timeout is expected (no message received)
    assert!(
        timeout.is_err(),
        "Should not receive message for other client"
    );

    write.close().await.ok();
}
