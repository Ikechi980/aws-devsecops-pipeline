//! Integration tests for yardi-sync.
//!
//! These tests require the following services to be running:
//! - resources-api (`cargo lambda watch -p resources-api --bin resources-api --invoke-address 127.0.0.1` in resources-api directory)
//! - mock-yardi-api (part of shared infrastructure)
//! - LocalStack (SNS/SQS)
//!
//! All tests run serially in a single test function to avoid race conditions
//! when modifying shared resources-api state.
//!
//! IMPORTANT: yardi-sync service must also be running for sync tests:
//!   cargo run

mod common;

use common::aws;
use common::*;
use std::env;
use std::time::Duration;

/// All integration tests run sequentially in this function.
/// This prevents race conditions from parallel access to resources-api.
#[tokio::test]
async fn integration_tests() {
    let ctx = TestContext::new().await;

    // Wait for infrastructure to be ready
    if let Err(e) = ctx.wait_for_infrastructure().await {
        eprintln!(
            "Infrastructure not ready: {}. Skipping integration tests.",
            e
        );
        eprintln!("Make sure to run: cd ../infra && docker compose up -d");
        eprintln!(
            "And: cd ../resources-api && cargo lambda watch -p resources-api --bin resources-api --invoke-address 127.0.0.1"
        );
        return;
    }

    // Reset mock Yardi state before tests
    if let Err(e) = ctx.mock_yardi.reset().await {
        eprintln!(
            "Failed to reset mock Yardi: {}. Skipping integration tests.",
            e
        );
        return;
    }

    println!("Running integration tests...");

    // === Infrastructure Health Tests ===
    test_resources_api_reachable(&ctx).await;
    test_mock_yardi_reachable(&ctx).await;
    require_yardi_sync_running(&ctx).await;

    // === Community Tracking Tests ===
    test_community_without_yardi_not_tracked(&ctx).await;
    test_community_with_yardi_created(&ctx).await;
    // Note: The following tests require yardi-sync to be running
    // They are structured to work both with and without the service
    test_community_yardi_integration_added(&ctx).await;
    test_community_yardi_integration_removed(&ctx).await;
    test_community_deleted(&ctx).await;

    // === Location Sync Tests ===
    test_yardi_location_synced(&ctx).await;
    test_yardi_location_name_update(&ctx).await;
    test_yardi_location_deleted(&ctx).await;
    test_location_deleted_externally_recovers(&ctx).await;
    test_all_location_types(&ctx).await;

    // === Resident Sync Tests ===
    test_yardi_resident_synced(&ctx).await;
    test_yardi_resident_encounter_hierarchy_uses_room_reference(&ctx).await;
    test_yardi_resident_uses_existing_room_when_trailing_location_is_missing(&ctx).await;
    test_yardi_resident_name_update(&ctx).await;
    test_yardi_resident_location_change(&ctx).await;
    test_paginated_yardi_bundle_fetches_all_pages(&ctx).await;
    test_yardi_resident_deleted(&ctx).await;
    test_planned_encounter_not_synced(&ctx).await;
    test_finished_encounter_not_synced(&ctx).await;
    test_in_progress_resident_without_location_publishes_failure(&ctx).await;
    test_onleave_resident_without_prior_location_is_skipped(&ctx).await;
    test_synced_resident_deleted_when_onleave_loses_room(&ctx).await;
    test_resident_location_not_in_resources_not_created(&ctx).await;
    test_multiple_encounters_uses_recent(&ctx).await;
    test_yardi_resident_photo_synced(&ctx).await;
    test_yardi_resident_photo_updates_and_removes(&ctx).await;
    test_yardi_resident_photo_skips_refetch_when_last_updated_unchanged(&ctx).await;
    test_yardi_resident_photo_invalid_base64_skips_photo(&ctx).await;

    // === Referential Integrity Tests ===
    test_delete_order_residents_before_locations(&ctx).await;
    test_create_order_locations_before_residents(&ctx).await;

    // === Yardi API Failure Tests ===
    test_yardi_invalid_credentials(&ctx).await;
    test_yardi_fhir_error(&ctx).await;
    test_yardi_patient_without_encounter_publishes_failure(&ctx).await;
    test_yardi_resident_without_room_publishes_failure(&ctx).await;
    test_yardi_location_missing_type_publishes_failure(&ctx).await;
    test_yardi_invalid_organization_id_publishes_failure(&ctx).await;
    test_yardi_failure_recovery(&ctx).await;

    // === Token Management Tests ===
    test_yardi_token_caching(&ctx).await;
    test_yardi_token_refresh_on_expiry(&ctx).await;
    test_yardi_token_invalidation_recovery(&ctx).await;

    // === Event Consumer Tests ===
    test_location_change_event_marks_dirty(&ctx).await;
    test_resident_change_event_marks_dirty(&ctx).await;
    test_malformed_sqs_message_handled_gracefully(&ctx).await;
    test_unknown_resource_type_ignored(&ctx).await;

    // === Edge Case Tests ===
    test_empty_yardi_org(&ctx).await;

    println!("All integration tests completed!");
}

// =============================================================================
// Infrastructure Health Tests
// =============================================================================

async fn test_resources_api_reachable(ctx: &TestContext) {
    println!("  test_resources_api_reachable...");
    let healthy = ctx.resources_api.health_check().await.unwrap();
    assert!(healthy, "resources-api should be reachable");
}

async fn test_mock_yardi_reachable(ctx: &TestContext) {
    println!("  test_mock_yardi_reachable...");
    let healthy = ctx.mock_yardi.health_check().await.unwrap();
    assert!(healthy, "mock-yardi-api should be reachable");
}

async fn require_yardi_sync_running(ctx: &TestContext) {
    println!("  require_yardi_sync_running...");

    let suffix = unique_suffix();
    let org_id = format!("org-required-{}", suffix);
    let api_key = format!("key-required-{}", suffix);
    let api_secret = format!("secret-required-{}", suffix);
    let loc_id = format!("loc-required-{}", suffix);
    let patient_id = format!("pat-required-{}", suffix);

    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_location(MockLocation::room(&loc_id, "Required Room"))
        .with_resident(&patient_id, "Required", "Resident", &loc_id);
    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Required Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    let result = wait_for_condition(sync_timeout(), || async {
        let locations = ctx.resources_api.list_locations(community.id).await?;
        let residents = ctx.resources_api.list_residents(community.id).await?;
        let location_ready = locations
            .iter()
            .any(|l| l.yardi_reference_id.as_deref() == Some(&loc_id));
        let resident_ready = residents
            .iter()
            .any(|r| r.yardi_reference_id.as_deref() == Some(&patient_id));
        Ok(location_ready && resident_ready)
    })
    .await;

    ctx.cleanup_community(community.id).await;

    if let Err(err) = result {
        panic!("yardi-sync must be running for tests to proceed: {}", err);
    }
}

async fn setup_failure_subscription() -> (aws::AwsTestClients, String, String) {
    let clients = aws::clients().await;
    let topic_arn = env::var("FAILURE_SNS_TOPIC_ARN")
        .expect("FAILURE_SNS_TOPIC_ARN must be set for SNS integration tests");
    let queue_name = format!("yardi-sync-failures-test-{}", unique_suffix());
    let queue_url = aws::create_queue(&clients.sqs, &queue_name)
        .await
        .expect("Failed to create SQS queue for failure notifications");
    let subscription_arn =
        aws::subscribe_queue_to_topic(&clients.sns, &clients.sqs, &topic_arn, &queue_url)
            .await
            .expect("Failed to subscribe test queue to failure topic");
    (clients, queue_url, subscription_arn)
}

async fn cleanup_failure_subscription(
    clients: &aws::AwsTestClients,
    queue_url: &str,
    subscription_arn: &str,
) {
    let _ = aws::unsubscribe(&clients.sns, subscription_arn).await;
    let _ = aws::delete_queue(&clients.sqs, queue_url).await;
}

async fn assert_failure_notification_delayed(clients: &aws::AwsTestClients, queue_url: &str) {
    tokio::time::sleep(sync_wait_time() * 4).await;
    aws::expect_no_sns_message(&clients.sqs, queue_url, Duration::from_secs(2))
        .await
        .expect("Failure notification should be held during the configured delay");
}

// =============================================================================
// Community Tracking Tests
// =============================================================================

async fn test_community_without_yardi_not_tracked(ctx: &TestContext) {
    println!("  test_community_without_yardi_not_tracked...");

    let suffix = unique_suffix();
    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::new(&format!(
            "No Yardi Community {}",
            suffix
        )))
        .await
        .expect("Failed to create community");

    // Verify community has no Yardi integration
    assert!(community.yardi_org_id.is_none());
    assert!(community.yardi_api_key.is_none());
    assert!(community.yardi_api_base_url.is_none());
    assert!(community.yardi_token_url.is_none());

    tokio::time::sleep(sync_wait_time()).await;

    let locations = ctx
        .resources_api
        .list_locations(community.id)
        .await
        .expect("Failed to list locations");
    let residents = ctx
        .resources_api
        .list_residents(community.id)
        .await
        .expect("Failed to list residents");
    assert!(locations.is_empty(), "No locations should be synced");
    assert!(residents.is_empty(), "No residents should be synced");

    // Cleanup
    ctx.cleanup_community(community.id).await;
}

async fn test_community_with_yardi_created(ctx: &TestContext) {
    println!("  test_community_with_yardi_created...");

    let suffix = unique_suffix();
    let org_id = format!("org-{}", suffix);
    let api_key = format!("key-{}", suffix);
    let api_secret = format!("secret-{}", suffix);
    let loc_id = format!("loc-{}", suffix);
    let patient_id = format!("pat-{}", suffix);

    // Configure mock Yardi with location and resident
    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_location(MockLocation::room(&loc_id, "Initial Room"))
        .with_resident(&patient_id, "Initial", "Resident", &loc_id);
    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    // Create community with Yardi credentials
    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Yardi Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    // Verify community has Yardi integration
    assert_eq!(community.yardi_org_id.as_deref(), Some(org_id.as_str()));
    assert_eq!(community.yardi_api_key.as_deref(), Some(api_key.as_str()));
    assert!(community.yardi_api_base_url.is_some());
    assert!(community.yardi_token_url.is_some());

    wait_for_condition(sync_timeout(), || async {
        let locations = ctx.resources_api.list_locations(community.id).await?;
        let residents = ctx.resources_api.list_residents(community.id).await?;
        let has_location = locations
            .iter()
            .any(|l| l.yardi_reference_id.as_deref() == Some(&loc_id));
        let has_resident = residents
            .iter()
            .any(|r| r.yardi_reference_id.as_deref() == Some(&patient_id));
        Ok(has_location && has_resident)
    })
    .await
    .expect("Expected initial sync after community creation");

    // Cleanup
    ctx.cleanup_community(community.id).await;
}

async fn test_community_yardi_integration_added(ctx: &TestContext) {
    println!("  test_community_yardi_integration_added...");

    let suffix = unique_suffix();
    let org_id = format!("org-add-{}", suffix);
    let api_key = format!("key-add-{}", suffix);
    let api_secret = format!("secret-add-{}", suffix);
    let patient_id = format!("pat-add-{}", suffix);

    // Configure mock Yardi with a location
    let loc_id = format!("loc-{}", suffix);
    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_location(MockLocation::room(&loc_id, "Test Room"))
        .with_resident(&patient_id, "Test", "Resident", &loc_id);
    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    // Create community WITHOUT Yardi
    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::new(&format!(
            "Integration Add Community {}",
            suffix
        )))
        .await
        .expect("Failed to create community");

    assert!(community.yardi_org_id.is_none());
    assert!(community.yardi_api_base_url.is_none());
    assert!(community.yardi_token_url.is_none());

    // Update community to ADD Yardi integration
    let updated = ctx
        .resources_api
        .update_community(
            community.id,
            &UpdateCommunity::with_yardi(
                &format!("Integration Add Community {}", suffix),
                &org_id,
                &api_key,
                &api_secret,
            ),
        )
        .await
        .expect("Failed to update community");

    assert_eq!(updated.yardi_org_id.as_deref(), Some(org_id.as_str()));
    assert!(updated.yardi_api_base_url.is_some());
    assert!(updated.yardi_token_url.is_some());

    wait_for_condition(sync_timeout(), || async {
        let locations = ctx.resources_api.list_locations(community.id).await?;
        let residents = ctx.resources_api.list_residents(community.id).await?;
        let has_location = locations
            .iter()
            .any(|l| l.yardi_reference_id.as_deref() == Some(&loc_id));
        let has_resident = residents
            .iter()
            .any(|r| r.yardi_reference_id.as_deref() == Some(&patient_id));
        Ok(has_location && has_resident)
    })
    .await
    .expect("Expected sync after integration added");

    // Cleanup
    ctx.cleanup_community(community.id).await;
}

async fn test_community_yardi_integration_removed(ctx: &TestContext) {
    println!("  test_community_yardi_integration_removed...");

    let suffix = unique_suffix();
    let org_id = format!("org-rem-{}", suffix);
    let api_key = format!("key-rem-{}", suffix);
    let api_secret = format!("secret-rem-{}", suffix);

    // Configure mock Yardi with a location and resident
    let loc_id = format!("loc-rem-{}", suffix);
    let patient_id = format!("pat-rem-{}", suffix);
    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_location(MockLocation::room(&loc_id, "Remove Room"))
        .with_resident(&patient_id, "Remove", "Resident", &loc_id);
    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    // Create community WITH Yardi
    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Integration Remove Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    wait_for_condition(sync_timeout(), || async {
        let locations = ctx.resources_api.list_locations(community.id).await?;
        let residents = ctx.resources_api.list_residents(community.id).await?;
        let has_location = locations
            .iter()
            .any(|l| l.yardi_reference_id.as_deref() == Some(&loc_id));
        let has_resident = residents
            .iter()
            .any(|r| r.yardi_reference_id.as_deref() == Some(&patient_id));
        Ok(has_location && has_resident)
    })
    .await
    .expect("Expected initial sync before removing integration");

    let locations = ctx
        .resources_api
        .list_locations(community.id)
        .await
        .expect("Failed to list locations");
    let residents = ctx
        .resources_api
        .list_residents(community.id)
        .await
        .expect("Failed to list residents");

    for resident in residents {
        ctx.resources_api
            .delete_resident(community.id, resident.id)
            .await
            .expect("Failed to delete resident before removing integration");
    }
    for location in locations {
        ctx.resources_api
            .delete_location(community.id, location.id)
            .await
            .expect("Failed to delete location before removing integration");
    }

    // Update community to REMOVE Yardi integration
    let updated = ctx
        .resources_api
        .update_community(
            community.id,
            &UpdateCommunity::new(&format!("Integration Remove Community {}", suffix)),
        )
        .await
        .expect("Failed to update community");

    assert!(updated.yardi_org_id.is_none());
    assert!(updated.yardi_api_base_url.is_none());
    assert!(updated.yardi_token_url.is_none());

    // Update mock Yardi with new data that should NOT be synced
    let new_locations = serde_json::json!({
        "resourceType": "Bundle",
        "entry": [{
            "resource": {
                "id": format!("loc-new-{}", suffix),
                "name": "Should Not Sync",
                "physicalType": { "coding": [{ "code": "ro" }] }
            }
        }]
    });
    ctx.mock_yardi
        .update_organization(&api_key, &org_id, Some(new_locations), None, None)
        .await
        .expect("Failed to update mock Yardi");

    tokio::time::sleep(sync_wait_time()).await;

    let locations_after = ctx
        .resources_api
        .list_locations(community.id)
        .await
        .expect("Failed to list locations after removal");
    assert!(
        locations_after.iter().all(|l| l.name != "Should Not Sync"),
        "New Yardi data should not be synced after integration removal"
    );

    // Cleanup
    ctx.cleanup_community(community.id).await;
}

async fn test_community_deleted(ctx: &TestContext) {
    println!("  test_community_deleted...");

    let suffix = unique_suffix();
    let org_id = format!("org-del-{}", suffix);
    let api_key = format!("key-del-{}", suffix);
    let api_secret = format!("secret-del-{}", suffix);

    // Configure mock Yardi
    let loc_id = format!("loc-del-{}", suffix);
    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_location(MockLocation::room(&loc_id, "Delete Room"));
    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    // Create and delete community
    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Delete Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    wait_for_condition(sync_timeout(), || async {
        let locations = ctx.resources_api.list_locations(community.id).await?;
        Ok(locations
            .iter()
            .any(|l| l.yardi_reference_id.as_deref() == Some(&loc_id)))
    })
    .await
    .expect("Expected initial sync before deletion");

    let locations = ctx
        .resources_api
        .list_locations(community.id)
        .await
        .expect("Failed to list locations before deletion");
    for location in locations {
        ctx.resources_api
            .delete_location(community.id, location.id)
            .await
            .expect("Failed to delete location before community delete");
    }

    ctx.resources_api
        .delete_community(community.id)
        .await
        .expect("Failed to delete community");

    // Verify community is gone
    let result = ctx.resources_api.get_community(community.id).await.unwrap();
    assert!(result.is_none());

    // Update mock Yardi; should not recreate anything for deleted community.
    let updated_locations = serde_json::json!({
        "resourceType": "Bundle",
        "entry": [{
            "resource": {
                "id": format!("loc-del-new-{}", suffix),
                "name": "Should Not Recreate",
                "physicalType": { "coding": [{ "code": "ro" }] }
            }
        }]
    });
    ctx.mock_yardi
        .update_organization(&api_key, &org_id, Some(updated_locations), None, None)
        .await
        .expect("Failed to update mock Yardi");

    tokio::time::sleep(sync_wait_time()).await;

    let locations = ctx
        .resources_api
        .list_locations_optional(community.id)
        .await
        .expect("Failed to list locations after delete");
    let residents = ctx
        .resources_api
        .list_residents_optional(community.id)
        .await
        .expect("Failed to list residents after delete");
    assert!(
        locations.is_none(),
        "Deleted community should have no locations"
    );
    assert!(
        residents.is_none(),
        "Deleted community should have no residents"
    );
}

// =============================================================================
// Location Sync Tests
// =============================================================================

async fn test_yardi_location_synced(ctx: &TestContext) {
    println!("  test_yardi_location_synced...");

    let suffix = unique_suffix();
    let org_id = format!("org-loc-{}", suffix);
    let api_key = format!("key-loc-{}", suffix);
    let api_secret = format!("secret-loc-{}", suffix);
    let loc_id = format!("loc-sync-{}", suffix);

    // Configure mock Yardi with a location
    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_location(MockLocation::room(&loc_id, "Synced Location"));
    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    // Create community
    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Location Sync Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    wait_for_condition(sync_timeout(), || async {
        let locations = ctx.resources_api.list_locations(community.id).await?;
        Ok(locations.iter().any(|l| {
            l.yardi_reference_id.as_deref() == Some(&loc_id) && l.name == "Synced Location"
        }))
    })
    .await
    .expect("Expected location sync from Yardi");

    // Cleanup
    ctx.cleanup_community(community.id).await;
}

async fn test_yardi_location_name_update(ctx: &TestContext) {
    println!("  test_yardi_location_name_update...");

    let suffix = unique_suffix();
    let org_id = format!("org-upd-{}", suffix);
    let api_key = format!("key-upd-{}", suffix);
    let api_secret = format!("secret-upd-{}", suffix);
    let loc_id = format!("loc-upd-{}", suffix);

    // Configure mock Yardi with original location name
    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_location(MockLocation::room(&loc_id, "Original Name"));
    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    // Create community and wait for sync
    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Location Update Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    tokio::time::sleep(sync_wait_time()).await;

    // Update location name in mock Yardi
    let new_locations = serde_json::json!({
        "resourceType": "Bundle",
        "entry": [{
            "resource": {
                "id": loc_id,
                "name": "Updated Name",
                "physicalType": {
                    "coding": [{ "code": "ro" }]
                }
            }
        }]
    });
    ctx.mock_yardi
        .update_organization(&api_key, &org_id, Some(new_locations), None, None)
        .await
        .expect("Failed to update mock Yardi");

    wait_for_condition(sync_timeout(), || async {
        let locations = ctx.resources_api.list_locations(community.id).await?;
        Ok(locations
            .iter()
            .any(|l| l.yardi_reference_id.as_deref() == Some(&loc_id) && l.name == "Updated Name"))
    })
    .await
    .expect("Expected location name update to sync");

    // Cleanup
    ctx.cleanup_community(community.id).await;
}

async fn test_yardi_location_deleted(ctx: &TestContext) {
    println!("  test_yardi_location_deleted...");

    let suffix = unique_suffix();
    let org_id = format!("org-ldel-{}", suffix);
    let api_key = format!("key-ldel-{}", suffix);
    let api_secret = format!("secret-ldel-{}", suffix);
    let loc_id = format!("loc-del-{}", suffix);

    // Configure mock Yardi with a location
    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_location(MockLocation::room(&loc_id, "To Be Deleted"));
    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    // Create community and wait for sync
    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Location Delete Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    tokio::time::sleep(sync_wait_time()).await;

    // Remove location from mock Yardi
    let empty_locations = serde_json::json!({
        "resourceType": "Bundle",
        "entry": []
    });
    ctx.mock_yardi
        .update_organization(&api_key, &org_id, Some(empty_locations), None, None)
        .await
        .expect("Failed to update mock Yardi");

    wait_for_condition(sync_timeout(), || async {
        let locations = ctx.resources_api.list_locations(community.id).await?;
        Ok(!locations
            .iter()
            .any(|l| l.yardi_reference_id.as_deref() == Some(&loc_id)))
    })
    .await
    .expect("Expected location deletion after Yardi removal");

    // Cleanup
    ctx.cleanup_community(community.id).await;
}

async fn test_location_deleted_externally_recovers(ctx: &TestContext) {
    println!("  test_location_deleted_externally_recovers...");

    let suffix = unique_suffix();
    let org_id = format!("org-race-{}", suffix);
    let api_key = format!("key-race-{}", suffix);
    let api_secret = format!("secret-race-{}", suffix);
    let loc_id = format!("loc-race-{}", suffix);

    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_location(MockLocation::room(&loc_id, "Race Room"));
    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Race Location Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    wait_for_condition(sync_timeout(), || async {
        let locations = ctx.resources_api.list_locations(community.id).await?;
        Ok(locations
            .iter()
            .any(|l| l.yardi_reference_id.as_deref() == Some(&loc_id)))
    })
    .await
    .expect("Expected initial location sync before external delete");

    let locations = ctx
        .resources_api
        .list_locations(community.id)
        .await
        .expect("Failed to list locations");
    let location = locations
        .iter()
        .find(|l| l.yardi_reference_id.as_deref() == Some(&loc_id))
        .expect("Expected synced location");

    ctx.resources_api
        .delete_location(community.id, location.id)
        .await
        .expect("Failed to delete location externally");

    let updated_locations = serde_json::json!({
        "resourceType": "Bundle",
        "entry": [{
            "resource": {
                "id": loc_id,
                "name": "Race Room Updated",
                "physicalType": { "coding": [{ "code": "ro" }] }
            }
        }]
    });
    ctx.mock_yardi
        .update_organization(&api_key, &org_id, Some(updated_locations), None, None)
        .await
        .expect("Failed to update mock Yardi");

    wait_for_condition(sync_timeout(), || async {
        let locations = ctx.resources_api.list_locations(community.id).await?;
        Ok(locations.iter().any(|l| {
            l.yardi_reference_id.as_deref() == Some(&loc_id) && l.name == "Race Room Updated"
        }))
    })
    .await
    .expect("Expected location to recover after external delete");

    ctx.cleanup_community(community.id).await;
}

async fn test_all_location_types(ctx: &TestContext) {
    println!("  test_all_location_types...");

    let suffix = unique_suffix();
    let org_id = format!("org-types-{}", suffix);
    let api_key = format!("key-types-{}", suffix);
    let api_secret = format!("secret-types-{}", suffix);

    // Configure mock Yardi with all location types
    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_location(MockLocation::site(&format!("site-{}", suffix), "Test Site"))
        .with_location(MockLocation::corridor(
            &format!("cor-{}", suffix),
            "Test Corridor",
            Some(&format!("site-{}", suffix)),
        ))
        .with_location(MockLocation::room(&format!("room-{}", suffix), "Test Room"))
        .with_location(MockLocation::bed(
            &format!("bed-{}", suffix),
            "Test Bed",
            Some(&format!("room-{}", suffix)),
        ));

    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    // Create community and wait for sync
    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("All Types Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    tokio::time::sleep(sync_wait_time()).await;

    wait_for_condition(sync_timeout(), || async {
        let locations = ctx.resources_api.list_locations(community.id).await?;
        Ok(locations.len() == 1
            && locations
                .iter()
                .any(|l| l.yardi_reference_id.as_deref() == Some(&format!("room-{}", suffix))))
    })
    .await
    .expect("Expected only room locations to sync");

    // Cleanup
    ctx.cleanup_community(community.id).await;
}

// =============================================================================
// Resident Sync Tests
// =============================================================================

async fn test_yardi_resident_synced(ctx: &TestContext) {
    println!("  test_yardi_resident_synced...");

    let suffix = unique_suffix();
    let org_id = format!("org-res-{}", suffix);
    let api_key = format!("key-res-{}", suffix);
    let api_secret = format!("secret-res-{}", suffix);
    let loc_id = format!("loc-res-{}", suffix);
    let patient_id = format!("pat-{}", suffix);

    // Configure mock Yardi with location and resident
    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_location(MockLocation::room(&loc_id, "Resident Room"))
        .with_resident(&patient_id, "John", "Doe", &loc_id);

    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    // Create community
    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Resident Sync Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    wait_for_condition(sync_timeout(), || async {
        let residents = ctx.resources_api.list_residents(community.id).await?;
        Ok(residents.iter().any(|r| {
            r.yardi_reference_id.as_deref() == Some(&patient_id)
                && r.first_name == "John"
                && r.last_name == "Doe"
        }))
    })
    .await
    .expect("Expected resident sync from Yardi");

    // Cleanup
    ctx.cleanup_community(community.id).await;
}

async fn test_yardi_resident_encounter_hierarchy_uses_room_reference(ctx: &TestContext) {
    println!("  test_yardi_resident_encounter_hierarchy_uses_room_reference...");

    let suffix = unique_suffix();
    let org_id = format!("org-room-ref-{}", suffix);
    let api_key = format!("key-room-ref-{}", suffix);
    let api_secret = format!("secret-room-ref-{}", suffix);
    let site_id = format!("site-{}", suffix);
    let corridor_id = format!("corridor-{}", suffix);
    let room_id = format!("room-{}", suffix);
    let bed_id = format!("bed-{}", suffix);
    let patient_id = format!("pat-{}", suffix);

    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_location(MockLocation::site(&site_id, "Site"))
        .with_location(MockLocation::corridor(
            &corridor_id,
            "Corridor",
            Some(&site_id),
        ))
        .with_location(MockLocation::room(&room_id, "Room"))
        .with_location(MockLocation::bed(&bed_id, "Bed", Some(&room_id)))
        .with_patient(MockPatient {
            id: patient_id.clone(),
            first_name: "Bed".to_string(),
            last_name: "Resident".to_string(),
            photo_content_type: None,
            photo_data_base64: None,
            last_updated: Some(chrono::Utc::now().to_rfc3339()),
        });

    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    let encounters = serde_json::json!({
        "resourceType": "Bundle",
        "entry": [{
            "resource": {
                "id": format!("enc-{}", patient_id),
                "status": "in-progress",
                "subject": {
                    "reference": format!("Patient/{}", patient_id)
                },
                "location": [
                    {
                        "location": {
                            "reference": format!("Location/{}", site_id)
                        }
                    },
                    {
                        "location": {
                            "reference": format!("Location/{}", corridor_id)
                        }
                    },
                    {
                        "location": {
                            "reference": format!("Location/{}", room_id)
                        }
                    },
                    {
                        "location": {
                            "reference": format!("Location/{}", bed_id)
                        }
                    }
                ],
                "period": {
                    "start": chrono::Utc::now().to_rfc3339()
                }
            }
        }]
    });

    ctx.mock_yardi
        .update_organization(&api_key, &org_id, None, None, Some(encounters))
        .await
        .expect("Failed to update mock Yardi encounters");

    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Bed Room Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    wait_for_condition(sync_timeout(), || async {
        let locations = ctx.resources_api.list_locations(community.id).await?;
        let residents = ctx.resources_api.list_residents(community.id).await?;
        let room = locations
            .iter()
            .find(|l| l.yardi_reference_id.as_deref() == Some(&room_id));
        let resident = residents
            .iter()
            .find(|r| r.yardi_reference_id.as_deref() == Some(&patient_id));

        Ok(match (room, resident) {
            (Some(room), Some(resident)) => resident.location_id == room.id && locations.len() == 1,
            _ => false,
        })
    })
    .await
    .expect("Expected resident to use the encounter-provided room reference");

    ctx.cleanup_community(community.id).await;
}

async fn test_yardi_resident_uses_existing_room_when_trailing_location_is_missing(
    ctx: &TestContext,
) {
    println!("  test_yardi_resident_uses_existing_room_when_trailing_location_is_missing...");

    let suffix = unique_suffix();
    let org_id = format!("org-room-fallback-{}", suffix);
    let api_key = format!("key-room-fallback-{}", suffix);
    let api_secret = format!("secret-room-fallback-{}", suffix);
    let room_id = format!("room-{}", suffix);
    let patient_id = format!("pat-room-fallback-{}", suffix);
    let missing_location_id = format!("missing-{}", suffix);

    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_location(MockLocation::room(&room_id, "Room"))
        .with_patient(MockPatient {
            id: patient_id.clone(),
            first_name: "Fallback".to_string(),
            last_name: "Resident".to_string(),
            photo_content_type: None,
            photo_data_base64: None,
            last_updated: Some(chrono::Utc::now().to_rfc3339()),
        });

    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    let encounters = serde_json::json!({
        "resourceType": "Bundle",
        "entry": [{
            "resource": {
                "id": format!("enc-{}", patient_id),
                "status": "in-progress",
                "subject": {
                    "reference": format!("Patient/{}", patient_id)
                },
                "location": [
                    {
                        "location": {
                            "reference": format!("Location/{}", room_id)
                        }
                    },
                    {
                        "location": {
                            "reference": format!("Location/{}", missing_location_id)
                        }
                    }
                ],
                "period": {
                    "start": chrono::Utc::now().to_rfc3339()
                }
            }
        }]
    });

    ctx.mock_yardi
        .update_organization(&api_key, &org_id, None, None, Some(encounters))
        .await
        .expect("Failed to update mock Yardi encounters");

    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Room Fallback Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    wait_for_condition(sync_timeout(), || async {
        let locations = ctx.resources_api.list_locations(community.id).await?;
        let residents = ctx.resources_api.list_residents(community.id).await?;
        let room = locations
            .iter()
            .find(|l| l.yardi_reference_id.as_deref() == Some(&room_id));
        let resident = residents
            .iter()
            .find(|r| r.yardi_reference_id.as_deref() == Some(&patient_id));

        Ok(match (room, resident) {
            (Some(room), Some(resident)) => resident.location_id == room.id && locations.len() == 1,
            _ => false,
        })
    })
    .await
    .expect("Expected resident to use the existing room location");

    ctx.cleanup_community(community.id).await;
}

async fn test_yardi_resident_name_update(ctx: &TestContext) {
    println!("  test_yardi_resident_name_update...");

    let suffix = unique_suffix();
    let org_id = format!("org-rname-{}", suffix);
    let api_key = format!("key-rname-{}", suffix);
    let api_secret = format!("secret-rname-{}", suffix);
    let loc_id = format!("loc-rname-{}", suffix);
    let patient_id = format!("pat-rname-{}", suffix);

    // Configure mock Yardi with resident
    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_location(MockLocation::room(&loc_id, "Name Change Room"))
        .with_resident(&patient_id, "Jane", "Doe", &loc_id);

    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    // Create community and wait for initial sync
    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Resident Name Update Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    tokio::time::sleep(sync_wait_time()).await;

    // Update resident name in mock Yardi
    let new_patients = serde_json::json!({
        "resourceType": "Bundle",
        "entry": [{
            "resource": {
                "id": patient_id,
                "name": [{
                    "use": "usual",
                    "family": "Smith",
                    "given": ["Jane"]
                }]
            }
        }]
    });
    ctx.mock_yardi
        .update_organization(&api_key, &org_id, None, Some(new_patients), None)
        .await
        .expect("Failed to update mock Yardi");

    wait_for_condition(sync_timeout(), || async {
        let residents = ctx.resources_api.list_residents(community.id).await?;
        Ok(residents.iter().any(|r| {
            r.yardi_reference_id.as_deref() == Some(&patient_id)
                && r.first_name == "Jane"
                && r.last_name == "Smith"
        }))
    })
    .await
    .expect("Expected resident name update to sync");

    // Cleanup
    ctx.cleanup_community(community.id).await;
}

async fn test_yardi_resident_location_change(ctx: &TestContext) {
    println!("  test_yardi_resident_location_change...");

    let suffix = unique_suffix();
    let org_id = format!("org-rloc-{}", suffix);
    let api_key = format!("key-rloc-{}", suffix);
    let api_secret = format!("secret-rloc-{}", suffix);
    let loc_a = format!("loc-a-{}", suffix);
    let loc_b = format!("loc-b-{}", suffix);
    let patient_id = format!("pat-rloc-{}", suffix);

    // Configure mock Yardi with 2 locations and resident in location A
    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_location(MockLocation::room(&loc_a, "Room A"))
        .with_location(MockLocation::room(&loc_b, "Room B"))
        .with_resident(&patient_id, "Bob", "Wilson", &loc_a);

    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    // Create community
    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Resident Location Change Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    tokio::time::sleep(sync_wait_time()).await;

    // Move resident to location B
    let new_encounters = serde_json::json!({
        "resourceType": "Bundle",
        "entry": [{
            "resource": {
                "id": format!("enc-{}", patient_id),
                "status": "in-progress",
                "subject": { "reference": format!("Patient/{}", patient_id) },
                "location": [{
                    "location": { "reference": format!("Location/{}", loc_b) }
                }],
                "period": { "start": chrono::Utc::now().to_rfc3339() }
            }
        }]
    });
    ctx.mock_yardi
        .update_organization(&api_key, &org_id, None, None, Some(new_encounters))
        .await
        .expect("Failed to update mock Yardi");

    wait_for_condition(sync_timeout(), || async {
        let residents = ctx.resources_api.list_residents(community.id).await?;
        let locations = ctx.resources_api.list_locations(community.id).await?;
        let resident = residents
            .iter()
            .find(|r| r.yardi_reference_id.as_deref() == Some(&patient_id));
        let loc_b_resource = locations
            .iter()
            .find(|l| l.yardi_reference_id.as_deref() == Some(&loc_b));

        Ok(match (resident, loc_b_resource) {
            (Some(res), Some(loc)) => res.location_id == loc.id,
            _ => false,
        })
    })
    .await
    .expect("Expected resident to move to location B");

    // Cleanup
    ctx.cleanup_community(community.id).await;
}

async fn test_paginated_yardi_bundle_fetches_all_pages(ctx: &TestContext) {
    println!("  test_paginated_yardi_bundle_fetches_all_pages...");

    let suffix = unique_suffix();
    let org_id = format!("org-paged-{}", suffix);
    let api_key = format!("key-paged-{}", suffix);
    let api_secret = format!("secret-paged-{}", suffix);
    let room_id = format!("room-paged-{}", suffix);
    let overflow_room_id = format!("room-paged-overflow-{}", suffix);

    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_locations_page_size(1)
        .with_patients_page_size(1)
        .with_encounters_page_size(1)
        .with_location(MockLocation::room(&room_id, "Paged Room"))
        .with_location(MockLocation::room(&overflow_room_id, "Overflow Room"))
        .with_patient(MockPatient {
            id: format!("resident-a-{}", suffix),
            first_name: "Paged".to_string(),
            last_name: "Alice".to_string(),
            photo_content_type: None,
            photo_data_base64: None,
            last_updated: Some(chrono::Utc::now().to_rfc3339()),
        })
        .with_patient(MockPatient {
            id: format!("resident-b-{}", suffix),
            first_name: "Paged".to_string(),
            last_name: "Bob".to_string(),
            photo_content_type: None,
            photo_data_base64: None,
            last_updated: Some(chrono::Utc::now().to_rfc3339()),
        })
        // Put Bob's encounter on page 1 and Alice's on page 2.
        .with_encounter(MockEncounter {
            id: format!("enc-b-{}", suffix),
            patient_id: format!("resident-b-{}", suffix),
            status: "in-progress".to_string(),
            location_id: Some(room_id.clone()),
            period_start: Some("2024-01-01T00:00:00Z".to_string()),
        })
        .with_encounter(MockEncounter {
            id: format!("enc-a-{}", suffix),
            patient_id: format!("resident-a-{}", suffix),
            status: "in-progress".to_string(),
            location_id: Some(room_id.clone()),
            period_start: Some("2024-02-01T00:00:00Z".to_string()),
        });
    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure paginated mock Yardi");

    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Paged Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    wait_for_condition(sync_timeout(), || async {
        let residents = ctx.resources_api.list_residents(community.id).await?;
        Ok(residents.len() == 2)
    })
    .await
    .expect("Expected both residents from paginated Yardi bundles");

    let residents = ctx
        .resources_api
        .list_residents(community.id)
        .await
        .expect("Failed to list residents");
    assert_eq!(residents.len(), 2);
    let locations = ctx
        .resources_api
        .list_locations(community.id)
        .await
        .expect("Failed to list locations");
    assert_eq!(locations.len(), 2);
    assert!(
        residents
            .iter()
            .any(|resident| resident.yardi_reference_id.as_deref()
                == Some(&format!("resident-a-{}", suffix)))
    );
    assert!(
        residents
            .iter()
            .any(|resident| resident.yardi_reference_id.as_deref()
                == Some(&format!("resident-b-{}", suffix)))
    );
    assert!(
        locations
            .iter()
            .any(|location| location.yardi_reference_id.as_deref() == Some(&room_id))
    );
    assert!(
        locations
            .iter()
            .any(|location| location.yardi_reference_id.as_deref() == Some(&overflow_room_id))
    );

    ctx.cleanup_community(community.id).await;
}

async fn test_yardi_resident_deleted(ctx: &TestContext) {
    println!("  test_yardi_resident_deleted...");

    let suffix = unique_suffix();
    let org_id = format!("org-rdel-{}", suffix);
    let api_key = format!("key-rdel-{}", suffix);
    let api_secret = format!("secret-rdel-{}", suffix);
    let loc_id = format!("loc-rdel-{}", suffix);
    let patient_id = format!("pat-rdel-{}", suffix);

    // Configure mock Yardi with resident
    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_location(MockLocation::room(&loc_id, "Delete Room"))
        .with_resident(&patient_id, "Delete", "Me", &loc_id);

    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    // Create community and wait for sync
    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Resident Delete Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    tokio::time::sleep(sync_wait_time()).await;

    // Remove patient and encounter from mock Yardi
    let empty_bundle = serde_json::json!({
        "resourceType": "Bundle",
        "entry": []
    });
    ctx.mock_yardi
        .update_organization(
            &api_key,
            &org_id,
            None,
            Some(empty_bundle.clone()),
            Some(empty_bundle),
        )
        .await
        .expect("Failed to update mock Yardi");

    wait_for_condition(sync_timeout(), || async {
        let residents = ctx.resources_api.list_residents(community.id).await?;
        Ok(!residents
            .iter()
            .any(|r| r.yardi_reference_id.as_deref() == Some(&patient_id)))
    })
    .await
    .expect("Expected resident deletion after Yardi removal");

    // Cleanup
    ctx.cleanup_community(community.id).await;
}

async fn test_planned_encounter_not_synced(ctx: &TestContext) {
    println!("  test_planned_encounter_not_synced...");

    let suffix = unique_suffix();
    let org_id = format!("org-plan-{}", suffix);
    let api_key = format!("key-plan-{}", suffix);
    let api_secret = format!("secret-plan-{}", suffix);
    let loc_id = format!("loc-plan-{}", suffix);
    let patient_id = format!("pat-plan-{}", suffix);

    // Configure mock Yardi with patient having "planned" encounter
    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_location(MockLocation::room(&loc_id, "Planned Room"))
        .with_patient(MockPatient {
            id: patient_id.clone(),
            first_name: "Planned".to_string(),
            last_name: "Patient".to_string(),
            photo_content_type: None,
            photo_data_base64: None,
            last_updated: Some(chrono::Utc::now().to_rfc3339()),
        })
        .with_encounter(MockEncounter::planned(
            &format!("enc-{}", suffix),
            &patient_id,
        ));

    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    // Create community and wait for sync
    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Planned Encounter Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    tokio::time::sleep(sync_wait_time()).await;

    // Verify resident was NOT synced
    let residents = ctx
        .resources_api
        .list_residents(community.id)
        .await
        .expect("Failed to list residents");

    let planned_res = residents
        .iter()
        .find(|r| r.yardi_reference_id.as_deref() == Some(&patient_id));
    assert!(
        planned_res.is_none(),
        "Patient with planned encounter should not be synced"
    );

    // Cleanup
    ctx.cleanup_community(community.id).await;
}

async fn test_finished_encounter_not_synced(ctx: &TestContext) {
    println!("  test_finished_encounter_not_synced...");

    let suffix = unique_suffix();
    let org_id = format!("org-fin-{}", suffix);
    let api_key = format!("key-fin-{}", suffix);
    let api_secret = format!("secret-fin-{}", suffix);
    let loc_id = format!("loc-fin-{}", suffix);
    let patient_id = format!("pat-fin-{}", suffix);

    // Configure mock Yardi with patient having "finished" encounter
    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_location(MockLocation::room(&loc_id, "Finished Room"))
        .with_patient(MockPatient {
            id: patient_id.clone(),
            first_name: "Finished".to_string(),
            last_name: "Patient".to_string(),
            photo_content_type: None,
            photo_data_base64: None,
            last_updated: Some(chrono::Utc::now().to_rfc3339()),
        })
        .with_encounter(MockEncounter::finished(
            &format!("enc-{}", suffix),
            &patient_id,
        ));

    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    // Create community and wait for sync
    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Finished Encounter Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    tokio::time::sleep(sync_wait_time()).await;

    // Verify resident was NOT synced
    let residents = ctx
        .resources_api
        .list_residents(community.id)
        .await
        .expect("Failed to list residents");

    let finished_res = residents
        .iter()
        .find(|r| r.yardi_reference_id.as_deref() == Some(&patient_id));
    assert!(
        finished_res.is_none(),
        "Patient with finished encounter should not be synced"
    );

    // Cleanup
    ctx.cleanup_community(community.id).await;
}

async fn test_in_progress_resident_without_location_publishes_failure(ctx: &TestContext) {
    println!("  test_in_progress_resident_without_location_publishes_failure...");

    let suffix = unique_suffix();
    let org_id = format!("org-noloc-{}", suffix);
    let api_key = format!("key-noloc-{}", suffix);
    let api_secret = format!("secret-noloc-{}", suffix);
    let patient_id = format!("pat-noloc-{}", suffix);
    let (clients, queue_url, subscription_arn) = setup_failure_subscription().await;

    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_patient(MockPatient {
            id: patient_id.clone(),
            first_name: "No".to_string(),
            last_name: "Location".to_string(),
            photo_content_type: None,
            photo_data_base64: None,
            last_updated: Some(chrono::Utc::now().to_rfc3339()),
        })
        .with_encounter(MockEncounter {
            id: format!("enc-{}", patient_id),
            patient_id: patient_id.clone(),
            status: "in-progress".to_string(),
            location_id: None,
            period_start: Some(chrono::Utc::now().to_rfc3339()),
        });

    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("No Location Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    assert_failure_notification_delayed(&clients, &queue_url).await;

    let residents = ctx
        .resources_api
        .list_residents(community.id)
        .await
        .expect("Failed to list residents");
    assert!(
        residents
            .iter()
            .all(|r| r.yardi_reference_id.as_deref() != Some(&patient_id)),
        "Resident without location should not be created"
    );

    ctx.cleanup_community(community.id).await;
    cleanup_failure_subscription(&clients, &queue_url, &subscription_arn).await;
}

async fn test_onleave_resident_without_prior_location_is_skipped(ctx: &TestContext) {
    println!("  test_onleave_resident_without_prior_location_is_skipped...");

    let suffix = unique_suffix();
    let org_id = format!("org-onleave-skip-{}", suffix);
    let api_key = format!("key-onleave-skip-{}", suffix);
    let api_secret = format!("secret-onleave-skip-{}", suffix);
    let patient_id = format!("pat-onleave-skip-{}", suffix);
    let (clients, queue_url, subscription_arn) = setup_failure_subscription().await;

    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_patient(MockPatient {
            id: patient_id.clone(),
            first_name: "Onleave".to_string(),
            last_name: "Skip".to_string(),
            photo_content_type: None,
            photo_data_base64: None,
            last_updated: Some(chrono::Utc::now().to_rfc3339()),
        })
        .with_encounter(MockEncounter {
            id: format!("enc-{}", patient_id),
            patient_id: patient_id.clone(),
            status: "onleave".to_string(),
            location_id: None,
            period_start: Some(chrono::Utc::now().to_rfc3339()),
        });

    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Onleave Skip Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    tokio::time::sleep(sync_wait_time()).await;

    aws::expect_no_sns_message(&clients.sqs, &queue_url, Duration::from_secs(2))
        .await
        .expect(
            "Onleave resident without prior room should be skipped without failure notification",
        );

    let residents = ctx
        .resources_api
        .list_residents(community.id)
        .await
        .expect("Failed to list residents");
    assert!(
        residents
            .iter()
            .all(|r| r.yardi_reference_id.as_deref() != Some(&patient_id)),
        "Skipped onleave resident should not be created"
    );

    ctx.cleanup_community(community.id).await;
    cleanup_failure_subscription(&clients, &queue_url, &subscription_arn).await;
}

async fn test_synced_resident_deleted_when_onleave_loses_room(ctx: &TestContext) {
    println!("  test_synced_resident_deleted_when_onleave_loses_room...");

    let suffix = unique_suffix();
    let org_id = format!("org-onleave-delete-{}", suffix);
    let api_key = format!("key-onleave-delete-{}", suffix);
    let api_secret = format!("secret-onleave-delete-{}", suffix);
    let room_id = format!("room-onleave-delete-{}", suffix);
    let patient_id = format!("pat-onleave-delete-{}", suffix);

    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_location(MockLocation::room(&room_id, "Delete Room"))
        .with_resident(&patient_id, "Delete", "Later", &room_id);

    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Onleave Delete Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    wait_for_condition(sync_timeout(), || async {
        let residents = ctx.resources_api.list_residents(community.id).await?;
        Ok(residents
            .iter()
            .any(|resident| resident.yardi_reference_id.as_deref() == Some(&patient_id)))
    })
    .await
    .expect("Expected resident to sync before onleave update");

    let onleave_encounters = serde_json::json!({
        "resourceType": "Bundle",
        "entry": [{
            "resource": {
                "id": format!("enc-{}", patient_id),
                "status": "onleave",
                "subject": {
                    "reference": format!("Patient/{}", patient_id)
                },
                "period": {
                    "start": (chrono::Utc::now() + chrono::Duration::minutes(1)).to_rfc3339()
                }
            }
        }]
    });

    ctx.mock_yardi
        .update_organization(&api_key, &org_id, None, None, Some(onleave_encounters))
        .await
        .expect("Failed to update mock Yardi encounters");

    wait_for_condition(sync_timeout(), || async {
        let residents = ctx.resources_api.list_residents(community.id).await?;
        Ok(residents
            .iter()
            .all(|resident| resident.yardi_reference_id.as_deref() != Some(&patient_id)))
    })
    .await
    .expect("Expected resident to be deleted after unrecoverable onleave state");

    ctx.cleanup_community(community.id).await;
}

async fn test_resident_location_not_in_resources_not_created(ctx: &TestContext) {
    println!("  test_resident_location_not_in_resources_not_created...");

    let suffix = unique_suffix();
    let org_id = format!("org-missingloc-{}", suffix);
    let api_key = format!("key-missingloc-{}", suffix);
    let api_secret = format!("secret-missingloc-{}", suffix);
    let patient_id = format!("pat-missingloc-{}", suffix);
    let missing_loc = format!("loc-missing-{}", suffix);

    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret).with_resident(
        &patient_id,
        "Missing",
        "Location",
        &missing_loc,
    );
    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Missing Location Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    tokio::time::sleep(sync_wait_time()).await;

    let residents = ctx
        .resources_api
        .list_residents(community.id)
        .await
        .expect("Failed to list residents");
    assert!(
        residents
            .iter()
            .all(|r| r.yardi_reference_id.as_deref() != Some(&patient_id)),
        "Resident with missing location should not be created"
    );

    ctx.cleanup_community(community.id).await;
}

async fn test_multiple_encounters_uses_recent(ctx: &TestContext) {
    println!("  test_multiple_encounters_uses_recent...");

    let suffix = unique_suffix();
    let org_id = format!("org-multi-{}", suffix);
    let api_key = format!("key-multi-{}", suffix);
    let api_secret = format!("secret-multi-{}", suffix);
    let loc_old = format!("loc-old-{}", suffix);
    let loc_new = format!("loc-new-{}", suffix);
    let patient_id = format!("pat-multi-{}", suffix);

    // Create timestamps
    let old_time = (chrono::Utc::now() - chrono::Duration::hours(2)).to_rfc3339();
    let new_time = chrono::Utc::now().to_rfc3339();

    // Configure mock Yardi with 2 encounters for same patient
    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_location(MockLocation::room(&loc_old, "Old Room"))
        .with_location(MockLocation::room(&loc_new, "New Room"))
        .with_patient(MockPatient {
            id: patient_id.clone(),
            first_name: "Multi".to_string(),
            last_name: "Encounter".to_string(),
            photo_content_type: None,
            photo_data_base64: None,
            last_updated: Some(chrono::Utc::now().to_rfc3339()),
        })
        .with_encounter(MockEncounter {
            id: format!("enc-old-{}", suffix),
            patient_id: patient_id.clone(),
            status: "in-progress".to_string(),
            location_id: Some(loc_old.clone()),
            period_start: Some(old_time),
        })
        .with_encounter(MockEncounter {
            id: format!("enc-new-{}", suffix),
            patient_id: patient_id.clone(),
            status: "in-progress".to_string(),
            location_id: Some(loc_new.clone()),
            period_start: Some(new_time),
        });

    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    // Create community and wait for sync
    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Multiple Encounters Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    tokio::time::sleep(sync_wait_time()).await;

    wait_for_condition(sync_timeout(), || async {
        let residents = ctx.resources_api.list_residents(community.id).await?;
        let locations = ctx.resources_api.list_locations(community.id).await?;
        let resident = residents
            .iter()
            .find(|r| r.yardi_reference_id.as_deref() == Some(&patient_id));
        let new_loc = locations
            .iter()
            .find(|l| l.yardi_reference_id.as_deref() == Some(&loc_new));
        Ok(match (resident, new_loc) {
            (Some(res), Some(loc)) => res.location_id == loc.id,
            _ => false,
        })
    })
    .await
    .expect("Expected resident to use most recent encounter");

    // Cleanup
    ctx.cleanup_community(community.id).await;
}

async fn test_yardi_resident_photo_synced(ctx: &TestContext) {
    println!("  test_yardi_resident_photo_synced...");

    let suffix = unique_suffix();
    let org_id = format!("org-photo-create-{}", suffix);
    let api_key = format!("key-photo-create-{}", suffix);
    let api_secret = format!("secret-photo-create-{}", suffix);
    let loc_id = format!("loc-photo-create-{}", suffix);
    let patient_id = format!("pat-photo-create-{}", suffix);
    let photo_data = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR4nGNgYAAAAAMAASsJTYQAAAAASUVORK5CYII=";

    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_location(MockLocation::room(&loc_id, "Photo Room"))
        .with_patient(MockPatient {
            id: patient_id.clone(),
            first_name: "Photo".to_string(),
            last_name: "Synced".to_string(),
            photo_content_type: Some("image/png".to_string()),
            photo_data_base64: Some(photo_data.to_string()),
            last_updated: Some(chrono::Utc::now().to_rfc3339()),
        })
        .with_encounter(MockEncounter {
            id: format!("enc-{}", patient_id),
            patient_id: patient_id.clone(),
            status: "in-progress".to_string(),
            location_id: Some(loc_id.clone()),
            period_start: Some(chrono::Utc::now().to_rfc3339()),
        });
    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Photo Sync Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    wait_for_condition(sync_timeout(), || async {
        let residents = ctx.resources_api.list_residents(community.id).await?;
        let resident = residents
            .iter()
            .find(|r| r.yardi_reference_id.as_deref() == Some(&patient_id));
        Ok(resident.and_then(|r| r.photo.as_ref()).is_some())
    })
    .await
    .expect("Expected resident photo to be synced");

    let resident = ctx
        .resources_api
        .list_residents(community.id)
        .await
        .expect("Failed to list residents")
        .into_iter()
        .find(|r| r.yardi_reference_id.as_deref() == Some(&patient_id))
        .expect("Expected resident to exist");
    let photo = resident.photo.expect("Expected resident photo metadata");
    assert!(photo.size_bytes > 0);
    assert!(!photo.updated_at.is_empty());

    ctx.cleanup_community(community.id).await;
}

async fn test_yardi_resident_photo_updates_and_removes(ctx: &TestContext) {
    println!("  test_yardi_resident_photo_updates_and_removes...");

    let suffix = unique_suffix();
    let org_id = format!("org-photo-update-{}", suffix);
    let api_key = format!("key-photo-update-{}", suffix);
    let api_secret = format!("secret-photo-update-{}", suffix);
    let loc_id = format!("loc-photo-update-{}", suffix);
    let patient_id = format!("pat-photo-update-{}", suffix);
    let photo_v1 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR4nGNgYAAAAAMAASsJTYQAAAAASUVORK5CYII=";
    let photo_v2 = "AQIDBAUGBwgJ";
    let updated_at_v1 = chrono::Utc::now().to_rfc3339();
    let updated_at_v2 = (chrono::Utc::now() + chrono::Duration::seconds(30)).to_rfc3339();

    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_location(MockLocation::room(&loc_id, "Photo Room"))
        .with_patient(MockPatient {
            id: patient_id.clone(),
            first_name: "Photo".to_string(),
            last_name: "Updated".to_string(),
            photo_content_type: Some("image/png".to_string()),
            photo_data_base64: Some(photo_v1.to_string()),
            last_updated: Some(updated_at_v1.clone()),
        })
        .with_encounter(MockEncounter {
            id: format!("enc-{}", patient_id),
            patient_id: patient_id.clone(),
            status: "in-progress".to_string(),
            location_id: Some(loc_id.clone()),
            period_start: Some(chrono::Utc::now().to_rfc3339()),
        });
    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Photo Update Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    wait_for_condition(sync_timeout(), || async {
        let residents = ctx.resources_api.list_residents(community.id).await?;
        let resident = residents
            .iter()
            .find(|r| r.yardi_reference_id.as_deref() == Some(&patient_id));
        Ok(resident.and_then(|r| r.photo.as_ref()).is_some())
    })
    .await
    .expect("Expected initial resident photo");

    let etag_v1 = ctx
        .resources_api
        .list_residents(community.id)
        .await
        .expect("Failed to list residents")
        .into_iter()
        .find(|r| r.yardi_reference_id.as_deref() == Some(&patient_id))
        .and_then(|r| r.photo.map(|p| p.etag))
        .expect("Expected resident photo etag");

    let updated_patients = serde_json::json!({
        "resourceType": "Bundle",
        "entry": [{
            "resource": {
                "id": patient_id,
                "meta": { "lastUpdated": updated_at_v2 },
                "name": [{ "use": "usual", "family": "Updated", "given": ["Photo"] }],
                "photo": [{
                    "contentType": "image/jpeg",
                    "data": photo_v2
                }]
            }
        }]
    });
    ctx.mock_yardi
        .update_organization(&api_key, &org_id, None, Some(updated_patients), None)
        .await
        .expect("Failed to update mock Yardi photo");

    wait_for_condition(sync_timeout(), || async {
        let residents = ctx.resources_api.list_residents(community.id).await?;
        let resident = residents
            .iter()
            .find(|r| r.yardi_reference_id.as_deref() == Some(&patient_id));
        Ok(resident
            .and_then(|r| r.photo.as_ref())
            .is_some_and(|photo| photo.content_type == "image/jpeg" && photo.etag != etag_v1))
    })
    .await
    .expect("Expected resident photo to update");

    let resident_after_update = ctx
        .resources_api
        .list_residents(community.id)
        .await
        .expect("Failed to list residents")
        .into_iter()
        .find(|r| r.yardi_reference_id.as_deref() == Some(&patient_id))
        .expect("Expected resident after photo update");
    let photo_after_update = resident_after_update
        .photo
        .expect("Expected photo after update");
    assert!(photo_after_update.size_bytes > 0);
    assert!(!photo_after_update.updated_at.is_empty());

    let no_photo_patients = serde_json::json!({
        "resourceType": "Bundle",
        "entry": [{
            "resource": {
                "id": patient_id,
                "meta": { "lastUpdated": chrono::Utc::now().to_rfc3339() },
                "name": [{ "use": "usual", "family": "Updated", "given": ["Photo"] }]
            }
        }]
    });
    ctx.mock_yardi
        .update_organization(&api_key, &org_id, None, Some(no_photo_patients), None)
        .await
        .expect("Failed to remove photo from mock Yardi");

    wait_for_condition(sync_timeout(), || async {
        let residents = ctx.resources_api.list_residents(community.id).await?;
        let resident = residents
            .iter()
            .find(|r| r.yardi_reference_id.as_deref() == Some(&patient_id));
        Ok(resident.is_some_and(|r| r.photo.is_none()))
    })
    .await
    .expect("Expected resident photo to be removed");

    ctx.cleanup_community(community.id).await;
}

async fn test_yardi_resident_photo_skips_refetch_when_last_updated_unchanged(ctx: &TestContext) {
    println!("  test_yardi_resident_photo_skips_refetch_when_last_updated_unchanged...");

    let suffix = unique_suffix();
    let org_id = format!("org-photo-cache-{}", suffix);
    let api_key = format!("key-photo-cache-{}", suffix);
    let api_secret = format!("secret-photo-cache-{}", suffix);
    let loc_id = format!("loc-photo-cache-{}", suffix);
    let patient_id = format!("pat-photo-cache-{}", suffix);
    let last_updated = chrono::Utc::now().to_rfc3339();

    ctx.mock_yardi
        .clear_requests()
        .await
        .expect("Failed to clear request log");

    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_location(MockLocation::room(&loc_id, "Photo Room"))
        .with_patient(MockPatient {
            id: patient_id.clone(),
            first_name: "Photo".to_string(),
            last_name: "Cached".to_string(),
            photo_content_type: Some("image/png".to_string()),
            photo_data_base64: Some("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR4nGNgYAAAAAMAASsJTYQAAAAASUVORK5CYII=".to_string()),
            last_updated: Some(last_updated),
        })
        .with_encounter(MockEncounter {
            id: format!("enc-{}", patient_id),
            patient_id: patient_id.clone(),
            status: "in-progress".to_string(),
            location_id: Some(loc_id.clone()),
            period_start: Some(chrono::Utc::now().to_rfc3339()),
        });
    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Photo Cache Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    wait_for_condition(sync_timeout(), || async {
        let residents = ctx.resources_api.list_residents(community.id).await?;
        let resident = residents
            .iter()
            .find(|r| r.yardi_reference_id.as_deref() == Some(&patient_id));
        Ok(resident.and_then(|r| r.photo.as_ref()).is_some())
    })
    .await
    .expect("Expected resident photo to be synced");

    ctx.mock_yardi
        .clear_requests()
        .await
        .expect("Failed to clear request log");

    tokio::time::sleep(sync_wait_time()).await;
    let baseline_requests = ctx
        .mock_yardi
        .get_requests(Some("fhir"))
        .await
        .expect("Failed to get mock request log");
    let baseline_detail_calls = baseline_requests
        .iter()
        .filter(|r| {
            r.get("resource").and_then(|v| v.as_str()) == Some("PatientDetail")
                && r.get("patientId").and_then(|v| v.as_str()) == Some(patient_id.as_str())
        })
        .count();

    tokio::time::sleep(sync_wait_time() * 2).await;

    let requests = ctx
        .mock_yardi
        .get_requests(Some("fhir"))
        .await
        .expect("Failed to get mock request log");
    let detail_calls = requests
        .iter()
        .filter(|r| {
            r.get("resource").and_then(|v| v.as_str()) == Some("PatientDetail")
                && r.get("patientId").and_then(|v| v.as_str()) == Some(patient_id.as_str())
        })
        .count();
    assert_eq!(
        detail_calls, baseline_detail_calls,
        "Patient detail should not be refetched when lastUpdated is unchanged"
    );

    ctx.cleanup_community(community.id).await;
}

async fn test_yardi_resident_photo_invalid_base64_skips_photo(ctx: &TestContext) {
    println!("  test_yardi_resident_photo_invalid_base64_skips_photo...");

    let suffix = unique_suffix();
    let org_id = format!("org-photo-b64-{}", suffix);
    let api_key = format!("key-photo-b64-{}", suffix);
    let api_secret = format!("secret-photo-b64-{}", suffix);
    let loc_id = format!("loc-photo-b64-{}", suffix);
    let patient_id = format!("pat-photo-b64-{}", suffix);

    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_location(MockLocation::room(&loc_id, "Photo Room"))
        .with_patient(MockPatient {
            id: patient_id.clone(),
            first_name: "Photo".to_string(),
            last_name: "Invalid".to_string(),
            photo_content_type: Some("image/png".to_string()),
            photo_data_base64: Some("not-base64!!!".to_string()),
            last_updated: Some(chrono::Utc::now().to_rfc3339()),
        })
        .with_encounter(MockEncounter {
            id: format!("enc-{}", patient_id),
            patient_id: patient_id.clone(),
            status: "in-progress".to_string(),
            location_id: Some(loc_id.clone()),
            period_start: Some(chrono::Utc::now().to_rfc3339()),
        });
    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Photo Invalid Base64 Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    wait_for_condition(sync_timeout(), || async {
        let residents = ctx.resources_api.list_residents(community.id).await?;
        Ok(residents
            .iter()
            .any(|r| r.yardi_reference_id.as_deref() == Some(&patient_id)))
    })
    .await
    .expect("Expected resident to be created despite invalid photo");

    let residents = ctx
        .resources_api
        .list_residents(community.id)
        .await
        .expect("Failed to list residents");
    let resident = residents
        .iter()
        .find(|r| r.yardi_reference_id.as_deref() == Some(&patient_id))
        .expect("Expected resident to be present");
    assert!(
        resident.photo.is_none(),
        "Resident photo should not be set when Yardi photo data is invalid base64"
    );

    ctx.cleanup_community(community.id).await;
}

// =============================================================================
// Referential Integrity Tests
// =============================================================================

async fn test_delete_order_residents_before_locations(ctx: &TestContext) {
    println!("  test_delete_order_residents_before_locations...");

    let suffix = unique_suffix();
    let org_id = format!("org-delord-{}", suffix);
    let api_key = format!("key-delord-{}", suffix);
    let api_secret = format!("secret-delord-{}", suffix);
    let loc_id = format!("loc-delord-{}", suffix);
    let patient_id = format!("pat-delord-{}", suffix);

    // Configure mock Yardi with location and resident
    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_location(MockLocation::room(&loc_id, "Delete Order Room"))
        .with_resident(&patient_id, "Delete", "Order", &loc_id);

    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    // Create community and wait for sync
    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Delete Order Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    tokio::time::sleep(sync_wait_time()).await;

    // Remove both location and resident from mock Yardi
    let empty_bundle = serde_json::json!({
        "resourceType": "Bundle",
        "entry": []
    });
    ctx.mock_yardi
        .update_organization(
            &api_key,
            &org_id,
            Some(empty_bundle.clone()),
            Some(empty_bundle.clone()),
            Some(empty_bundle),
        )
        .await
        .expect("Failed to update mock Yardi");

    tokio::time::sleep(sync_wait_time()).await;

    wait_for_condition(sync_timeout(), || async {
        let residents = ctx.resources_api.list_residents(community.id).await?;
        let locations = ctx.resources_api.list_locations(community.id).await?;
        let resident_deleted = residents
            .iter()
            .all(|r| r.yardi_reference_id.as_deref() != Some(&patient_id));
        let location_deleted = locations
            .iter()
            .all(|l| l.yardi_reference_id.as_deref() != Some(&loc_id));
        Ok(resident_deleted && location_deleted)
    })
    .await
    .expect("Expected resident and location deletion to complete");

    // Cleanup
    ctx.cleanup_community(community.id).await;
}

async fn test_create_order_locations_before_residents(ctx: &TestContext) {
    println!("  test_create_order_locations_before_residents...");

    let suffix = unique_suffix();
    let org_id = format!("org-creord-{}", suffix);
    let api_key = format!("key-creord-{}", suffix);
    let api_secret = format!("secret-creord-{}", suffix);
    let loc_id = format!("loc-creord-{}", suffix);
    let patient_id = format!("pat-creord-{}", suffix);

    // Configure mock Yardi with EMPTY data initially
    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret);
    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    // Create community (no data to sync)
    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Create Order Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    tokio::time::sleep(sync_wait_time()).await;

    // Now add both location and resident simultaneously
    let locations = serde_json::json!({
        "resourceType": "Bundle",
        "entry": [{
            "resource": {
                "id": loc_id,
                "name": "Create Order Room",
                "physicalType": { "coding": [{ "code": "ro" }] }
            }
        }]
    });
    let patients = serde_json::json!({
        "resourceType": "Bundle",
        "entry": [{
            "resource": {
                "id": patient_id,
                "name": [{ "use": "usual", "family": "Order", "given": ["Create"] }]
            }
        }]
    });
    let encounters = serde_json::json!({
        "resourceType": "Bundle",
        "entry": [{
            "resource": {
                "id": format!("enc-{}", patient_id),
                "status": "in-progress",
                "subject": { "reference": format!("Patient/{}", patient_id) },
                "location": [{ "location": { "reference": format!("Location/{}", loc_id) } }],
                "period": { "start": chrono::Utc::now().to_rfc3339() }
            }
        }]
    });

    ctx.mock_yardi
        .update_organization(
            &api_key,
            &org_id,
            Some(locations),
            Some(patients),
            Some(encounters),
        )
        .await
        .expect("Failed to update mock Yardi");

    wait_for_condition(sync_timeout(), || async {
        let residents = ctx.resources_api.list_residents(community.id).await?;
        let locs = ctx.resources_api.list_locations(community.id).await?;
        let resident = residents
            .iter()
            .find(|r| r.yardi_reference_id.as_deref() == Some(&patient_id));
        let location = locs
            .iter()
            .find(|l| l.yardi_reference_id.as_deref() == Some(&loc_id));
        Ok(match (resident, location) {
            (Some(res), Some(loc)) => res.location_id == loc.id,
            _ => false,
        })
    })
    .await
    .expect("Expected locations before residents when creating");

    // Cleanup
    ctx.cleanup_community(community.id).await;
}

// =============================================================================
// Yardi API Failure Tests
// =============================================================================

async fn test_yardi_invalid_credentials(ctx: &TestContext) {
    println!("  test_yardi_invalid_credentials...");

    let suffix = unique_suffix();
    let org_id = format!("org-invcred-{}", suffix);
    let (clients, queue_url, subscription_arn) = setup_failure_subscription().await;

    // Create community with credentials that DON'T exist in mock
    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Invalid Creds Community {}", suffix),
            &org_id,
            "wrong-key",
            "wrong-secret",
        ))
        .await
        .expect("Failed to create community");

    assert_failure_notification_delayed(&clients, &queue_url).await;

    // Cleanup
    ctx.cleanup_community(community.id).await;
    cleanup_failure_subscription(&clients, &queue_url, &subscription_arn).await;
}

async fn test_yardi_fhir_error(ctx: &TestContext) {
    println!("  test_yardi_fhir_error...");

    let suffix = unique_suffix();
    let org_id = format!("org-fhirerr-{}", suffix);
    let api_key = format!("key-fhirerr-{}", suffix);
    let api_secret = format!("secret-fhirerr-{}", suffix);
    let (clients, queue_url, subscription_arn) = setup_failure_subscription().await;

    // Configure mock Yardi to return 500 for FHIR requests
    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret);
    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    ctx.mock_yardi
        .set_failures(&FailureConfig::fhir_internal_error())
        .await
        .expect("Failed to set failures");

    // Create community
    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("FHIR Error Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    assert_failure_notification_delayed(&clients, &queue_url).await;

    // Clear the failure
    ctx.mock_yardi
        .clear_failures()
        .await
        .expect("Failed to clear failures");

    // Cleanup
    ctx.cleanup_community(community.id).await;
    cleanup_failure_subscription(&clients, &queue_url, &subscription_arn).await;
}

async fn test_yardi_patient_without_encounter_publishes_failure(ctx: &TestContext) {
    println!("  test_yardi_patient_without_encounter_publishes_failure...");

    let suffix = unique_suffix();
    let org_id = format!("org-noenc-{}", suffix);
    let api_key = format!("key-noenc-{}", suffix);
    let api_secret = format!("secret-noenc-{}", suffix);
    let patient_id = format!("pat-noenc-{}", suffix);
    let (clients, queue_url, subscription_arn) = setup_failure_subscription().await;

    let config =
        OrganizationConfig::new(&org_id, &api_key, &api_secret).with_patient(MockPatient {
            id: patient_id,
            first_name: "No".to_string(),
            last_name: "Encounter".to_string(),
            photo_content_type: None,
            photo_data_base64: None,
            last_updated: Some(chrono::Utc::now().to_rfc3339()),
        });
    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("No Encounter Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    assert_failure_notification_delayed(&clients, &queue_url).await;

    ctx.cleanup_community(community.id).await;
    cleanup_failure_subscription(&clients, &queue_url, &subscription_arn).await;
}

async fn test_yardi_resident_without_room_publishes_failure(ctx: &TestContext) {
    println!("  test_yardi_resident_without_room_publishes_failure...");

    let suffix = unique_suffix();
    let org_id = format!("org-noroom-{}", suffix);
    let api_key = format!("key-noroom-{}", suffix);
    let api_secret = format!("secret-noroom-{}", suffix);
    let site_id = format!("site-{}", suffix);
    let corridor_id = format!("cor-{}", suffix);
    let patient_id = format!("pat-noroom-{}", suffix);
    let (clients, queue_url, subscription_arn) = setup_failure_subscription().await;

    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_location(MockLocation::site(&site_id, "Site"))
        .with_location(MockLocation::corridor(
            &corridor_id,
            "Corridor",
            Some(&site_id),
        ))
        .with_resident(&patient_id, "No", "Room", &corridor_id);
    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("No Room Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    assert_failure_notification_delayed(&clients, &queue_url).await;

    ctx.cleanup_community(community.id).await;
    cleanup_failure_subscription(&clients, &queue_url, &subscription_arn).await;
}

async fn test_yardi_location_missing_type_publishes_failure(ctx: &TestContext) {
    println!("  test_yardi_location_missing_type_publishes_failure...");

    let suffix = unique_suffix();
    let org_id = format!("org-notype-{}", suffix);
    let api_key = format!("key-notype-{}", suffix);
    let api_secret = format!("secret-notype-{}", suffix);
    let loc_id = format!("loc-notype-{}", suffix);
    let (clients, queue_url, subscription_arn) = setup_failure_subscription().await;

    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret);
    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    let bad_locations = serde_json::json!({
        "resourceType": "Bundle",
        "entry": [{
            "resource": {
                "id": loc_id,
                "name": "Missing Type"
            }
        }]
    });
    ctx.mock_yardi
        .update_organization(&api_key, &org_id, Some(bad_locations), None, None)
        .await
        .expect("Failed to update mock Yardi");

    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Missing Type Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    assert_failure_notification_delayed(&clients, &queue_url).await;

    ctx.cleanup_community(community.id).await;
    cleanup_failure_subscription(&clients, &queue_url, &subscription_arn).await;
}

async fn test_yardi_invalid_organization_id_publishes_failure(ctx: &TestContext) {
    println!("  test_yardi_invalid_organization_id_publishes_failure...");

    let suffix = unique_suffix();
    let valid_org_id = format!("org-valid-{}", suffix);
    let invalid_org_id = format!("org-invalid-{}", suffix);
    let api_key = format!("key-badorg-{}", suffix);
    let api_secret = format!("secret-badorg-{}", suffix);
    let loc_id = format!("loc-badorg-{}", suffix);
    let (clients, queue_url, subscription_arn) = setup_failure_subscription().await;

    let config = OrganizationConfig::new(&valid_org_id, &api_key, &api_secret)
        .with_location(MockLocation::room(&loc_id, "Bad Org Room"));
    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Invalid Org Community {}", suffix),
            &invalid_org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    assert_failure_notification_delayed(&clients, &queue_url).await;

    ctx.cleanup_community(community.id).await;
    cleanup_failure_subscription(&clients, &queue_url, &subscription_arn).await;
}

async fn test_yardi_failure_recovery(ctx: &TestContext) {
    println!("  test_yardi_failure_recovery...");

    let suffix = unique_suffix();
    let org_id = format!("org-recov-{}", suffix);
    let api_key = format!("key-recov-{}", suffix);
    let api_secret = format!("secret-recov-{}", suffix);
    let loc_id = format!("loc-recov-{}", suffix);
    let (clients, queue_url, subscription_arn) = setup_failure_subscription().await;

    // Configure mock Yardi with data but make it fail initially
    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_location(MockLocation::room(&loc_id, "Recovery Room"));
    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    ctx.mock_yardi
        .set_failures(&FailureConfig::fhir_internal_error())
        .await
        .expect("Failed to set failures");

    // Create community
    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Recovery Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    assert_failure_notification_delayed(&clients, &queue_url).await;

    // Verify no locations synced
    let locations = ctx
        .resources_api
        .list_locations(community.id)
        .await
        .expect("Failed to list locations");
    assert!(
        !locations
            .iter()
            .any(|l| l.yardi_reference_id.as_deref() == Some(&loc_id)),
        "Location should not be synced during failure"
    );

    tokio::time::sleep(sync_wait_time()).await;

    // Clear the failure and wait for recovery
    ctx.mock_yardi
        .clear_failures()
        .await
        .expect("Failed to clear failures");
    wait_for_condition(sync_timeout(), || async {
        let locations = ctx.resources_api.list_locations(community.id).await?;
        Ok(locations
            .iter()
            .any(|l| l.yardi_reference_id.as_deref() == Some(&loc_id)))
    })
    .await
    .expect("Expected sync after recovery");

    ctx.mock_yardi
        .set_failures(&FailureConfig::fhir_internal_error())
        .await
        .expect("Failed to set failures");
    assert_failure_notification_delayed(&clients, &queue_url).await;

    // Cleanup
    ctx.mock_yardi
        .clear_failures()
        .await
        .expect("Failed to clear failures");
    ctx.cleanup_community(community.id).await;
    cleanup_failure_subscription(&clients, &queue_url, &subscription_arn).await;
}

// =============================================================================
// Token Management Tests
// =============================================================================

async fn test_yardi_token_caching(ctx: &TestContext) {
    println!("  test_yardi_token_caching...");

    let suffix = unique_suffix();
    let org_id = format!("org-cache-{}", suffix);
    let api_key = format!("key-cache-{}", suffix);
    let api_secret = format!("secret-cache-{}", suffix);

    // Clear request log
    ctx.mock_yardi
        .clear_requests()
        .await
        .expect("Failed to clear requests");

    // Configure mock Yardi
    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_location(MockLocation::room(&format!("loc-{}", suffix), "Cache Room"));
    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    // Create community
    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Token Cache Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    wait_for_condition(sync_timeout(), || async {
        let locations = ctx.resources_api.list_locations(community.id).await?;
        Ok(!locations.is_empty())
    })
    .await
    .expect("Expected initial sync before checking token cache");

    // Wait for multiple sync cycles
    tokio::time::sleep(sync_wait_time() * 2).await;

    // Check token requests (should be minimal due to caching)
    let token_requests = ctx
        .mock_yardi
        .get_requests(Some("token"))
        .await
        .expect("Failed to get requests");
    let token_requests_for_key = token_requests
        .iter()
        .filter(|req| req.get("apiKey").and_then(|v| v.as_str()) == Some(api_key.as_str()))
        .count();

    assert!(
        token_requests_for_key >= 1,
        "Expected token requests to be recorded for the test api key"
    );
    assert!(
        token_requests_for_key <= 2,
        "Expected token caching to limit token requests, got {}",
        token_requests_for_key
    );

    // Cleanup
    ctx.cleanup_community(community.id).await;
}

async fn test_yardi_token_refresh_on_expiry(ctx: &TestContext) {
    println!("  test_yardi_token_refresh_on_expiry...");

    let suffix = unique_suffix();
    let org_id = format!("org-expiry-{}", suffix);
    let api_key = format!("key-expiry-{}", suffix);
    let api_secret = format!("secret-expiry-{}", suffix);

    ctx.mock_yardi
        .clear_requests()
        .await
        .expect("Failed to clear requests");

    // Configure mock Yardi with very short token TTL
    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_token_ttl(2) // 2 second TTL
        .with_location(MockLocation::room(
            &format!("loc-{}", suffix),
            "Expiry Room",
        ));
    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    // Create community
    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Token Expiry Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    wait_for_condition(sync_timeout(), || async {
        let locations = ctx.resources_api.list_locations(community.id).await?;
        Ok(!locations.is_empty())
    })
    .await
    .expect("Expected initial sync before token expiry test");

    // Wait for token to expire
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Wait for another sync (should get new token)
    tokio::time::sleep(sync_wait_time()).await;

    wait_for_condition(sync_timeout(), || async {
        let token_requests = ctx.mock_yardi.get_requests(Some("token")).await?;
        let token_requests_for_key = token_requests
            .iter()
            .filter(|req| req.get("apiKey").and_then(|v| v.as_str()) == Some(api_key.as_str()))
            .count();
        Ok(token_requests_for_key >= 2)
    })
    .await
    .expect("Expected token refresh after expiry");

    // Cleanup
    ctx.cleanup_community(community.id).await;
}

async fn test_yardi_token_invalidation_recovery(ctx: &TestContext) {
    println!("  test_yardi_token_invalidation_recovery...");

    let suffix = unique_suffix();
    let org_id = format!("org-inv-{}", suffix);
    let api_key = format!("key-inv-{}", suffix);
    let api_secret = format!("secret-inv-{}", suffix);

    ctx.mock_yardi
        .clear_requests()
        .await
        .expect("Failed to clear requests");

    // Configure mock Yardi
    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret).with_location(
        MockLocation::room(&format!("loc-{}", suffix), "Invalidation Room"),
    );
    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    // Create community and wait for initial sync
    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Token Invalidation Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    wait_for_condition(sync_timeout(), || async {
        let locations = ctx.resources_api.list_locations(community.id).await?;
        Ok(!locations.is_empty())
    })
    .await
    .expect("Expected initial sync before token invalidation test");

    // Invalidate all tokens for this API key
    ctx.mock_yardi
        .invalidate_tokens(&api_key)
        .await
        .expect("Failed to invalidate tokens");

    // Wait for sync (should get new token automatically)
    tokio::time::sleep(sync_wait_time()).await;

    wait_for_condition(sync_timeout(), || async {
        let token_requests = ctx.mock_yardi.get_requests(Some("token")).await?;
        let token_requests_for_key = token_requests
            .iter()
            .filter(|req| req.get("apiKey").and_then(|v| v.as_str()) == Some(api_key.as_str()))
            .count();
        Ok(token_requests_for_key >= 2)
    })
    .await
    .expect("Expected token refresh after invalidation");

    // Cleanup
    ctx.cleanup_community(community.id).await;
}

// =============================================================================
// Event Consumer Tests
// =============================================================================

async fn test_location_change_event_marks_dirty(ctx: &TestContext) {
    println!("  test_location_change_event_marks_dirty...");

    let suffix = unique_suffix();
    let org_id = format!("org-evt-loc-{}", suffix);
    let api_key = format!("key-evt-loc-{}", suffix);
    let api_secret = format!("secret-evt-loc-{}", suffix);
    let location_ref = format!("external-loc-{}", suffix);
    let yardi_loc_id = format!("yardi-loc-{}", suffix);

    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_location(MockLocation::room(&yardi_loc_id, "Yardi Room"));
    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Event Location Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    wait_for_condition(sync_timeout(), || async {
        let locations = ctx.resources_api.list_locations(community.id).await?;
        Ok(locations
            .iter()
            .any(|l| l.yardi_reference_id.as_deref() == Some(&yardi_loc_id)))
    })
    .await
    .expect("Expected initial Yardi location sync before event test");

    let location = ctx
        .resources_api
        .create_location(community.id, "External Location", &location_ref)
        .await
        .expect("Failed to create external location");

    wait_for_condition(sync_timeout(), || async {
        let locations = ctx.resources_api.list_locations(community.id).await?;
        Ok(!locations.iter().any(|l| l.id == location.id))
    })
    .await
    .expect("Expected out-of-band location to be removed after event");

    ctx.cleanup_community(community.id).await;
}

async fn test_resident_change_event_marks_dirty(ctx: &TestContext) {
    println!("  test_resident_change_event_marks_dirty...");

    let suffix = unique_suffix();
    let org_id = format!("org-evt-res-{}", suffix);
    let api_key = format!("key-evt-res-{}", suffix);
    let api_secret = format!("secret-evt-res-{}", suffix);
    let loc_id = format!("loc-evt-res-{}", suffix);
    let resident_ref = format!("external-res-{}", suffix);

    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_location(MockLocation::room(&loc_id, "Event Room"));
    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Event Resident Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    wait_for_condition(sync_timeout(), || async {
        let locations = ctx.resources_api.list_locations(community.id).await?;
        Ok(locations
            .iter()
            .any(|l| l.yardi_reference_id.as_deref() == Some(&loc_id)))
    })
    .await
    .expect("Expected initial location sync before resident event");

    let locations = ctx
        .resources_api
        .list_locations(community.id)
        .await
        .expect("Failed to list locations");
    let location = locations
        .iter()
        .find(|l| l.yardi_reference_id.as_deref() == Some(&loc_id))
        .expect("Expected synced location");

    let resident = ctx
        .resources_api
        .create_resident(
            community.id,
            location.id,
            "External",
            "Resident",
            &resident_ref,
        )
        .await
        .expect("Failed to create external resident");

    wait_for_condition(sync_timeout(), || async {
        let residents = ctx.resources_api.list_residents(community.id).await?;
        Ok(!residents.iter().any(|r| r.id == resident.id))
    })
    .await
    .expect("Expected out-of-band resident to be removed after event");

    ctx.cleanup_community(community.id).await;
}

async fn test_malformed_sqs_message_handled_gracefully(ctx: &TestContext) {
    println!("  test_malformed_sqs_message_handled_gracefully...");

    let queue_url = env::var("RESOURCES_EVENTS_QUEUE_URL")
        .expect("RESOURCES_EVENTS_QUEUE_URL must be set for SQS tests");
    let clients = aws::clients().await;
    aws::send_raw_message(&clients.sqs, &queue_url, "not-json")
        .await
        .expect("Failed to send malformed SQS message");

    let suffix = unique_suffix();
    let org_id = format!("org-badmsg-{}", suffix);
    let api_key = format!("key-badmsg-{}", suffix);
    let api_secret = format!("secret-badmsg-{}", suffix);
    let location_ref = format!("external-badmsg-{}", suffix);
    let yardi_loc_id = format!("yardi-badmsg-{}", suffix);

    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_location(MockLocation::room(&yardi_loc_id, "Yardi Room"));
    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Malformed Message Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    wait_for_condition(sync_timeout(), || async {
        let locations = ctx.resources_api.list_locations(community.id).await?;
        Ok(locations
            .iter()
            .any(|l| l.yardi_reference_id.as_deref() == Some(&yardi_loc_id)))
    })
    .await
    .expect("Expected initial Yardi location sync before malformed message test");

    let location = ctx
        .resources_api
        .create_location(community.id, "External Location", &location_ref)
        .await
        .expect("Failed to create external location");

    wait_for_condition(sync_timeout(), || async {
        let locations = ctx.resources_api.list_locations(community.id).await?;
        Ok(!locations.iter().any(|l| l.id == location.id))
    })
    .await
    .expect("Expected consumer to process events after malformed message");

    ctx.cleanup_community(community.id).await;
}

async fn test_unknown_resource_type_ignored(ctx: &TestContext) {
    println!("  test_unknown_resource_type_ignored...");

    let queue_url = env::var("RESOURCES_EVENTS_QUEUE_URL")
        .expect("RESOURCES_EVENTS_QUEUE_URL must be set for SQS tests");
    let clients = aws::clients().await;
    let unknown_event = serde_json::json!({
        "resource_type": "unknown_resource",
        "event_type": "create",
        "after": {
            "community_id": uuid::Uuid::new_v4().to_string()
        }
    });
    let envelope = serde_json::json!({ "Message": unknown_event.to_string() });
    aws::send_raw_message(&clients.sqs, &queue_url, &envelope.to_string())
        .await
        .expect("Failed to send unknown resource type message");

    let suffix = unique_suffix();
    let org_id = format!("org-unknown-{}", suffix);
    let api_key = format!("key-unknown-{}", suffix);
    let api_secret = format!("secret-unknown-{}", suffix);
    let loc_id = format!("loc-unknown-{}", suffix);
    let resident_ref = format!("external-unknown-{}", suffix);

    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_location(MockLocation::room(&loc_id, "Unknown Room"));
    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Unknown Event Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    wait_for_condition(sync_timeout(), || async {
        let locations = ctx.resources_api.list_locations(community.id).await?;
        Ok(locations
            .iter()
            .any(|l| l.yardi_reference_id.as_deref() == Some(&loc_id)))
    })
    .await
    .expect("Expected initial sync before unknown event test");

    let locations = ctx
        .resources_api
        .list_locations(community.id)
        .await
        .expect("Failed to list locations");
    let location = locations
        .iter()
        .find(|l| l.yardi_reference_id.as_deref() == Some(&loc_id))
        .expect("Expected synced location");

    let resident = ctx
        .resources_api
        .create_resident(
            community.id,
            location.id,
            "External",
            "Resident",
            &resident_ref,
        )
        .await
        .expect("Failed to create external resident");

    wait_for_condition(sync_timeout(), || async {
        let residents = ctx.resources_api.list_residents(community.id).await?;
        Ok(!residents.iter().any(|r| r.id == resident.id))
    })
    .await
    .expect("Expected consumer to ignore unknown message and process later events");

    ctx.cleanup_community(community.id).await;
}

// =============================================================================
// Edge Case Tests
// =============================================================================

async fn test_empty_yardi_org(ctx: &TestContext) {
    println!("  test_empty_yardi_org...");

    let suffix = unique_suffix();
    let org_id = format!("org-empty-{}", suffix);
    let api_key = format!("key-empty-{}", suffix);
    let api_secret = format!("secret-empty-{}", suffix);

    // Configure mock Yardi with data, then clear it
    let loc_id = format!("loc-empty-{}", suffix);
    let patient_id = format!("pat-empty-{}", suffix);
    let config = OrganizationConfig::new(&org_id, &api_key, &api_secret)
        .with_location(MockLocation::room(&loc_id, "Empty Room"))
        .with_resident(&patient_id, "Empty", "Resident", &loc_id);
    ctx.mock_yardi
        .create_organization(&config)
        .await
        .expect("Failed to configure mock Yardi");

    // Create community
    let community = ctx
        .resources_api
        .create_community(&CreateCommunity::with_yardi(
            &format!("Empty Org Community {}", suffix),
            &org_id,
            &api_key,
            &api_secret,
        ))
        .await
        .expect("Failed to create community");

    wait_for_condition(sync_timeout(), || async {
        let locations = ctx.resources_api.list_locations(community.id).await?;
        let residents = ctx.resources_api.list_residents(community.id).await?;
        Ok(locations
            .iter()
            .any(|l| l.yardi_reference_id.as_deref() == Some(&loc_id))
            && residents
                .iter()
                .any(|r| r.yardi_reference_id.as_deref() == Some(&patient_id)))
    })
    .await
    .expect("Expected initial sync before clearing Yardi data");

    let empty_bundle = serde_json::json!({
        "resourceType": "Bundle",
        "entry": []
    });
    ctx.mock_yardi
        .update_organization(
            &api_key,
            &org_id,
            Some(empty_bundle.clone()),
            Some(empty_bundle.clone()),
            Some(empty_bundle),
        )
        .await
        .expect("Failed to clear mock Yardi data");

    wait_for_condition(sync_timeout(), || async {
        let locations = ctx.resources_api.list_locations(community.id).await?;
        let residents = ctx.resources_api.list_residents(community.id).await?;
        Ok(locations.is_empty() && residents.is_empty())
    })
    .await
    .expect("Expected empty Yardi org to delete existing data");

    // Cleanup
    ctx.cleanup_community(community.id).await;
}
