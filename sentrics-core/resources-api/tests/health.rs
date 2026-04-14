#[tokio::test]
async fn test_health_endpoint() {
    let client = reqwest::Client::new();

    let response = client
        .get("http://127.0.0.1:9000/lambda-url/resources-api/v1/health")
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert_eq!(body["status"], "ok");
}
