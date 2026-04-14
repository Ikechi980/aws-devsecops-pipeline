use reqwest::{Client, Method, StatusCode};

const BASE_URL: &str = "http://127.0.0.1:9000/lambda-url/resources-api";

#[tokio::test]
async fn test_cors_headers() {
    let client = Client::new();

    // Test a simple GET request for CORS headers
    let response = client
        .get(format!("{}/v1/health", BASE_URL))
        .header("Origin", "http://example.com")
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("access-control-allow-origin")
            .unwrap(),
        "http://example.com"
    );
    // Test OPTIONS preflight for health endpoint (Should only allow GET)
    let response = client
        .request(Method::OPTIONS, format!("{}/v1/health", BASE_URL))
        .header("Origin", "http://example.com")
        .header("Access-Control-Request-Method", "GET")
        .send()
        .await
        .expect("Failed to send health preflight request");

    assert_eq!(response.status(), StatusCode::OK);
    let allowed_methods = response
        .headers()
        .get("access-control-allow-methods")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(allowed_methods.contains("GET"));
    assert!(!allowed_methods.contains("POST"));
    assert!(!allowed_methods.contains("DELETE"));

    // Test OPTIONS preflight request for communities (Should allow all)
    let response = client
        .request(Method::OPTIONS, format!("{}/v1/communities", BASE_URL))
        .header("Origin", "http://example.com")
        .header("Access-Control-Request-Method", "POST")
        .send()
        .await
        .expect("Failed to send preflight request");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("access-control-allow-origin")
            .unwrap(),
        "http://example.com"
    );
    let allowed_methods = response
        .headers()
        .get("access-control-allow-methods")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        allowed_methods.contains("POST"),
        "Expected allowed_methods to contain 'POST', but got: '{}'",
        allowed_methods
    );

    // Verify GET is also allowed
    let response = client
        .request(Method::OPTIONS, format!("{}/v1/communities", BASE_URL))
        .header("Origin", "http://example.com")
        .header("Access-Control-Request-Method", "GET")
        .send()
        .await
        .expect("Failed to send preflight request for GET");

    assert_eq!(response.status(), StatusCode::OK);
    let allowed_methods = response
        .headers()
        .get("access-control-allow-methods")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(allowed_methods.contains("GET"));

    // Verify DELETE is also allowed
    let response = client
        .request(Method::OPTIONS, format!("{}/v1/communities", BASE_URL))
        .header("Origin", "http://example.com")
        .header("Access-Control-Request-Method", "DELETE")
        .send()
        .await
        .expect("Failed to send preflight request for DELETE");

    assert_eq!(response.status(), StatusCode::OK);
    let allowed_methods = response
        .headers()
        .get("access-control-allow-methods")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(allowed_methods.contains("DELETE"));
}
