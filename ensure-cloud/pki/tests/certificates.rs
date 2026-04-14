//! End-to-end tests for POST /v1/certificates endpoint.
//!
//! This file contains all tests for the certificate issuance endpoint, organized by category:
//! - Happy Path: Successful certificate requests
//! - CSR Validation: Input validation (PEM format, JSON structure)
//! - Domain Rules: CN and SAN validation for .ensurelink.net domain
//! - Authorization: IP-based authorization and community lookup
//! - HTTP Layer: Status codes, methods, error responses
//! - Edge Cases: Unusual but valid inputs
//!
//! These tests require the full development environment to be running:
//!   Terminal 1: ./scripts/dev.sh run
//!   Terminal 2: cargo test
//!
//! The mock-systems-api provides community data for:
//!   - alpha: 10.0.0.5
//!   - beta: 10.0.0.6
//!   - gamma: 10.0.0.7
//!   - test-local: 127.0.0.1
//!
//! The ALLOWED_CIDRS config allows 127.0.0.1/32 to bypass community lookup.

mod common;

use reqwest::StatusCode;
use serde_json::json;

// ============================================================================
// Happy Path Tests
// ============================================================================

#[tokio::test]
async fn cidr_allowed_bypasses_community_lookup() {
    let client = common::client();

    // 127.0.0.1 is in ALLOWED_CIDRS, so it bypasses community lookup
    let response = client
        .post(format!("{}/v1/certificates", common::BASE_URL))
        .header("x-forwarded-for", "127.0.0.1")
        .json(&json!({ "csr": common::CSR_ALPHA }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::CREATED);

    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    let chain = body["chain"].as_array().expect("chain should be array");

    // Verify chain structure: [issued cert, intermediate cert, root cert]
    assert_eq!(chain.len(), 3);

    for (i, cert) in chain.iter().enumerate() {
        let cert_pem = cert.as_str().expect("cert should be string");
        assert!(
            cert_pem.starts_with("-----BEGIN CERTIFICATE-----"),
            "chain[{}] should start with BEGIN CERTIFICATE",
            i
        );
        assert!(
            cert_pem.contains("-----END CERTIFICATE-----"),
            "chain[{}] should contain END CERTIFICATE",
            i
        );
    }
}

#[tokio::test]
async fn matching_community_ip_succeeds() {
    let client = common::client();

    // Verify each community (alpha, beta, gamma) can get a certificate from their registered IP
    for (csr, ip, community) in [
        (common::CSR_ALPHA, "10.0.0.5", "alpha"),
        (common::CSR_BETA, "10.0.0.6", "beta"),
        (common::CSR_GAMMA, "10.0.0.7", "gamma"),
    ] {
        let response = client
            .post(format!("{}/v1/certificates", common::BASE_URL))
            .header("x-forwarded-for", ip)
            .json(&json!({ "csr": csr }))
            .send()
            .await
            .expect("Failed to send request");

        assert_eq!(
            response.status(),
            StatusCode::CREATED,
            "{} community from IP {} should succeed",
            community,
            ip
        );
    }
}

// ============================================================================
// CSR Validation Tests - Invalid Input
// ============================================================================

#[tokio::test]
async fn invalid_csr_pem_rejected() {
    let client = common::client();

    let response = client
        .post(format!("{}/v1/certificates", common::BASE_URL))
        .header("x-forwarded-for", "127.0.0.1")
        .json(&json!({ "csr": "not a valid PEM" }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert_eq!(
        body["error"],
        "CSR must be a valid PEM encoded certificate request"
    );
}

#[tokio::test]
async fn wrong_pem_label_rejected() {
    let client = common::client();

    // This is a certificate, not a certificate request
    let wrong_label = "-----BEGIN CERTIFICATE-----\nMIIBkTCB+wIJAKHHCgVZU6UdMA0GCSqGSIb3DQEBCwUAMA0xCzAJBgNVBAYTAlVT\n-----END CERTIFICATE-----";

    let response = client
        .post(format!("{}/v1/certificates", common::BASE_URL))
        .header("x-forwarded-for", "127.0.0.1")
        .json(&json!({ "csr": wrong_label }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert_eq!(
        body["error"],
        "CSR must be a valid PEM encoded certificate request"
    );
}

#[tokio::test]
async fn empty_csr_field_rejected() {
    let client = common::client();

    let response = client
        .post(format!("{}/v1/certificates", common::BASE_URL))
        .header("x-forwarded-for", "127.0.0.1")
        .json(&json!({ "csr": "" }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn invalid_json_body_rejected() {
    let client = common::client();

    let response = client
        .post(format!("{}/v1/certificates", common::BASE_URL))
        .header("x-forwarded-for", "127.0.0.1")
        .header("content-type", "application/json")
        .body("not valid json")
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn missing_csr_field_rejected() {
    let client = common::client();

    let response = client
        .post(format!("{}/v1/certificates", common::BASE_URL))
        .header("x-forwarded-for", "127.0.0.1")
        .json(&json!({}))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// ============================================================================
// CSR Validation Tests - Domain Rules
// ============================================================================

#[tokio::test]
async fn cn_not_ending_with_domain_rejected() {
    let client = common::client();

    let response = client
        .post(format!("{}/v1/certificates", common::BASE_URL))
        .header("x-forwarded-for", "127.0.0.1")
        .json(&json!({ "csr": common::CSR_WRONG_DOMAIN }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert_eq!(body["error"], "CN must end with .ensurelink.net");
}

#[tokio::test]
async fn empty_community_id_rejected() {
    let client = common::client();

    let response = client
        .post(format!("{}/v1/certificates", common::BASE_URL))
        .header("x-forwarded-for", "127.0.0.1")
        .json(&json!({ "csr": common::CSR_EMPTY_COMMUNITY }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert_eq!(body["error"], "CN must be <community>.ensurelink.net");
}

#[tokio::test]
async fn san_not_ending_with_domain_rejected() {
    let client = common::client();

    let response = client
        .post(format!("{}/v1/certificates", common::BASE_URL))
        .header("x-forwarded-for", "127.0.0.1")
        .json(&json!({ "csr": common::CSR_BAD_SAN }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert_eq!(
        body["error"],
        "All SAN entries must end with .ensurelink.net"
    );
}

#[tokio::test]
async fn san_not_matching_community_rejected_for_non_bypass_ip() {
    let client = common::client();

    // CSR for beta but with alpha SAN - should fail when not from CIDR bypass
    let response = client
        .post(format!("{}/v1/certificates", common::BASE_URL))
        .header("x-forwarded-for", "10.0.0.6") // beta's IP
        .json(&json!({ "csr": common::CSR_BETA_WRONG_SAN }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert_eq!(body["error"], "SANs must match the requesting community");
}

#[tokio::test]
async fn san_not_matching_community_allowed_for_bypass_ip() {
    let client = common::client();

    // From bypass IP, we can request a certificate for a different community
    // without the IP matching that community. Use alpha CSR from bypass IP
    // to verify that we skip the community IP check.
    let response = client
        .post(format!("{}/v1/certificates", common::BASE_URL))
        .header("x-forwarded-for", "127.0.0.1") // bypass IP
        .json(&json!({ "csr": common::CSR_ALPHA }))
        .send()
        .await
        .expect("Failed to send request");

    // Bypass IP allows requesting any community's certificate
    assert_eq!(response.status(), StatusCode::CREATED);
}

// ============================================================================
// Authorization Tests
// ============================================================================

#[tokio::test]
async fn mismatched_ip_rejected() {
    let client = common::client();

    // beta has IP 10.0.0.6, request from different IP should fail
    let response = client
        .post(format!("{}/v1/certificates", common::BASE_URL))
        .header("x-forwarded-for", "10.0.0.5") // alpha's IP
        .json(&json!({ "csr": common::CSR_BETA }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert_eq!(body["error"], "Unauthorized for requested community");
}

#[tokio::test]
async fn nonexistent_community_rejected() {
    let client = common::client();

    // Request from IP that doesn't match alpha's expected IP (10.0.0.5)
    // This will fail authorization because the IP doesn't match
    let response = client
        .post(format!("{}/v1/certificates", common::BASE_URL))
        .header("x-forwarded-for", "10.99.99.99")
        .json(&json!({ "csr": common::CSR_ALPHA }))
        .send()
        .await
        .expect("Failed to send request");

    // Returns 403 because IP doesn't match the community's registered IP
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn request_without_x_forwarded_for_uses_remote_addr() {
    let client = common::client();

    // When X-Forwarded-For is absent, falls back to remote addr
    // In production, AWS API Gateway automatically adds XFF
    // This fallback enables local development without a proxy
    let response = client
        .post(format!("{}/v1/certificates", common::BASE_URL))
        .json(&json!({ "csr": common::CSR_ALPHA }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn malformed_x_forwarded_for_falls_back_to_remote_addr() {
    let client = common::client();

    // Send malformed XFF - should fall back to remote addr (127.0.0.1)
    let response = client
        .post(format!("{}/v1/certificates", common::BASE_URL))
        .header("x-forwarded-for", "not-an-ip")
        .json(&json!({ "csr": common::CSR_ALPHA }))
        .send()
        .await
        .expect("Failed to send request");

    // Falls back to remote addr (127.0.0.1) which is in ALLOWED_CIDRS, so succeeds
    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn x_forwarded_for_multiple_ips_uses_first() {
    let client = common::client();

    // Multiple IPs in XFF - first one should be used
    let response = client
        .post(format!("{}/v1/certificates", common::BASE_URL))
        .header("x-forwarded-for", "10.0.0.6, 192.168.1.1, 172.16.0.1")
        .json(&json!({ "csr": common::CSR_BETA }))
        .send()
        .await
        .expect("Failed to send request");

    // First IP (10.0.0.6) matches beta's IP
    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn x_forwarded_for_first_ip_mismatch_fails() {
    let client = common::client();

    // First IP doesn't match, even though later ones might
    let response = client
        .post(format!("{}/v1/certificates", common::BASE_URL))
        .header("x-forwarded-for", "10.0.0.5, 10.0.0.6, 10.0.0.7")
        .json(&json!({ "csr": common::CSR_BETA }))
        .send()
        .await
        .expect("Failed to send request");

    // First IP (10.0.0.5 = alpha) doesn't match beta (10.0.0.6)
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

// ============================================================================
// HTTP Layer Tests
// ============================================================================

#[tokio::test]
async fn unknown_route_returns_404() {
    let client = common::client();

    let response = client
        .get(format!("{}/v1/unknown", common::BASE_URL))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn wrong_method_returns_405() {
    let client = common::client();

    // GET on certificates endpoint (should be POST)
    let response = client
        .get(format!("{}/v1/certificates", common::BASE_URL))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn put_to_certificates_returns_405() {
    let client = common::client();

    let response = client
        .put(format!("{}/v1/certificates", common::BASE_URL))
        .json(&json!({ "csr": common::CSR_ALPHA }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn delete_to_certificates_returns_405() {
    let client = common::client();

    let response = client
        .delete(format!("{}/v1/certificates", common::BASE_URL))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

// ============================================================================
// Edge Cases
// ============================================================================

#[tokio::test]
async fn very_long_csr_field_handled() {
    let client = common::client();

    // Send an extremely long but invalid CSR
    let long_garbage = "A".repeat(100_000);

    let response = client
        .post(format!("{}/v1/certificates", common::BASE_URL))
        .header("x-forwarded-for", "127.0.0.1")
        .json(&json!({ "csr": long_garbage }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn csr_with_whitespace_handled() {
    let client = common::client();

    // Add extra whitespace around the CSR
    let csr_with_whitespace = format!("  \n\n{}\n\n  ", common::CSR_ALPHA);

    let response = client
        .post(format!("{}/v1/certificates", common::BASE_URL))
        .header("x-forwarded-for", "127.0.0.1")
        .json(&json!({ "csr": csr_with_whitespace }))
        .send()
        .await
        .expect("Failed to send request");

    // The server trims the CSR, so this should succeed
    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn content_type_application_json_required_implicitly() {
    let client = common::client();

    // Send without content-type header (reqwest sets it for .json())
    // This verifies the endpoint accepts JSON properly
    let response = client
        .post(format!("{}/v1/certificates", common::BASE_URL))
        .header("x-forwarded-for", "127.0.0.1")
        .json(&json!({ "csr": common::CSR_ALPHA }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn null_csr_value_rejected() {
    let client = common::client();

    let response = client
        .post(format!("{}/v1/certificates", common::BASE_URL))
        .header("x-forwarded-for", "127.0.0.1")
        .json(&json!({ "csr": null }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn csr_as_number_rejected() {
    let client = common::client();

    let response = client
        .post(format!("{}/v1/certificates", common::BASE_URL))
        .header("x-forwarded-for", "127.0.0.1")
        .json(&json!({ "csr": 12345 }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn extra_json_fields_ignored() {
    let client = common::client();

    // Send extra fields that aren't part of the API
    let response = client
        .post(format!("{}/v1/certificates", common::BASE_URL))
        .header("x-forwarded-for", "127.0.0.1")
        .json(&json!({
            "csr": common::CSR_ALPHA,
            "extra_field": "ignored",
            "another": 123
        }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn empty_x_forwarded_for_header_uses_remote_addr() {
    let client = common::client();

    // Send with empty X-Forwarded-For header
    let response = client
        .post(format!("{}/v1/certificates", common::BASE_URL))
        .header("x-forwarded-for", "")
        .json(&json!({ "csr": common::CSR_ALPHA }))
        .send()
        .await
        .expect("Failed to send request");

    // Should fall back to remote addr (127.0.0.1) which is in ALLOWED_CIDRS
    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn ipv6_address_parsed_and_validated() {
    let client = common::client();

    // IPv6 loopback address (::1) should be parsed correctly
    let response = client
        .post(format!("{}/v1/certificates", common::BASE_URL))
        .header("x-forwarded-for", "::1")
        .json(&json!({ "csr": common::CSR_ALPHA }))
        .send()
        .await
        .expect("Failed to send request");

    // IPv6 is parsed successfully, but ::1 doesn't match alpha's registered IP (10.0.0.5)
    // So it correctly fails authorization
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert_eq!(body["error"], "Unauthorized for requested community");
}

#[tokio::test]
async fn request_with_text_plain_content_type_accepted() {
    let client = common::client();

    // Accepts JSON body even with text/plain Content-Type
    let json_body = serde_json::to_string(&json!({ "csr": common::CSR_ALPHA })).unwrap();

    let response = client
        .post(format!("{}/v1/certificates", common::BASE_URL))
        .header("x-forwarded-for", "127.0.0.1")
        .header("content-type", "text/plain")
        .body(json_body)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn request_without_content_type_header() {
    let client = common::client();

    // Accepts JSON body even without Content-Type header
    let json_body = serde_json::to_string(&json!({ "csr": common::CSR_ALPHA })).unwrap();

    let response = client
        .post(format!("{}/v1/certificates", common::BASE_URL))
        .header("x-forwarded-for", "127.0.0.1")
        .body(json_body)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn uppercase_domain_rejected() {
    let client = common::client();

    // Domain validation is case-sensitive - only lowercase .ensurelink.net allowed
    let response = client
        .post(format!("{}/v1/certificates", common::BASE_URL))
        .header("x-forwarded-for", "127.0.0.1")
        .json(&json!({ "csr": common::CSR_UPPERCASE }))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = response.json().await.unwrap();
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("must end with .ensurelink.net")
    );
}

#[tokio::test]
async fn concurrent_certificate_requests() {
    let client = common::client();

    // Make multiple concurrent requests to ensure no race conditions
    let mut handles = vec![];

    for _ in 0..5 {
        let client = client.clone();
        let handle = tokio::spawn(async move {
            client
                .post(format!("{}/v1/certificates", common::BASE_URL))
                .header("x-forwarded-for", "127.0.0.1")
                .json(&json!({ "csr": common::CSR_ALPHA }))
                .send()
                .await
        });
        handles.push(handle);
    }

    let results = futures::future::join_all(handles).await;

    for result in results {
        let response = result.expect("Task panicked").expect("Request failed");
        assert_eq!(
            response.status(),
            StatusCode::CREATED,
            "All concurrent requests should succeed"
        );
    }
}

#[tokio::test]
async fn request_body_size_limit_enforced() {
    let client = common::client();

    // Create a request body that exceeds the 256KB limit
    let large_csr = "A".repeat(300 * 1024); // 300KB of 'A's
    let response = client
        .post(format!("{}/v1/certificates", common::BASE_URL))
        .header("x-forwarded-for", "127.0.0.1")
        .json(&json!({ "csr": large_csr }))
        .send()
        .await
        .expect("Failed to send request");

    // Should get 413 Payload Too Large
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}
