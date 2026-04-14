mod common;

#[tokio::test]
async fn health_ok() {
    let url = format!("{}/v1/health", common::base_url());
    let response = reqwest::get(url).await.expect("health request failed");
    assert!(response.status().is_success());
}
