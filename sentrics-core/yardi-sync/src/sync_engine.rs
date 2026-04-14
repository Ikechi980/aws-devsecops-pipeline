use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use base64::Engine;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::models::{
    CommunityState, FailureType, Location, Resident, YardiLocation, YardiLocationType,
    YardiResident,
};
use crate::resources_client::{ResourcesApiClient, ResourcesApiError};
use crate::state::StateManager;
use crate::timeouts::FAILURE_NOTIFICATION_HOLD;
use crate::yardi_client::{YardiClient, YardiError};

#[derive(Default)]
struct SyncStats {
    created: usize,
    updated: usize,
    deleted: usize,
}

enum PhotoSyncError {
    Yardi(YardiError),
    Other(anyhow::Error),
}

impl SyncStats {
    fn has_changes(&self) -> bool {
        self.created > 0 || self.updated > 0 || self.deleted > 0
    }
}

/// Synchronizes Yardi data with resources-api for a single community.
pub struct SyncEngine {
    yardi_client: Arc<YardiClient>,
    resources_client: Arc<ResourcesApiClient>,
    state_manager: Arc<StateManager>,
}

impl SyncEngine {
    pub fn new(
        yardi_client: Arc<YardiClient>,
        resources_client: Arc<ResourcesApiClient>,
        state_manager: Arc<StateManager>,
    ) -> Self {
        Self {
            yardi_client,
            resources_client,
            state_manager,
        }
    }

    /// Performs a full sync for a community: fetches data from both sources,
    /// computes differences, and applies changes to resources-api.
    pub async fn sync_community(
        &self,
        community_id: Uuid,
        resources_refresh_interval: Duration,
    ) -> Result<()> {
        let Some(tracked) = self.state_manager.get_community(community_id) else {
            tracing::debug!(community_id = %community_id, "Community not tracked, skipping sync");
            return Ok(());
        };

        let Some(credentials) = tracked.community.yardi_credentials() else {
            tracing::debug!(community_id = %community_id, "Community has no Yardi credentials");
            return Ok(());
        };

        tracing::debug!(
            community_id = %community_id,
            community_name = %tracked.community.name,
            "Starting sync"
        );

        if let Err(e) = self
            .yardi_client
            .validate_organization_id(&credentials)
            .await
        {
            self.handle_yardi_error(&e, community_id, &tracked.community.name)
                .await;
            return Ok(());
        }

        // Fetch Yardi data
        let yardi_locations = match self.yardi_client.fetch_locations(&credentials).await {
            Ok(locations) => {
                self.state_manager
                    .clear_failure(FailureType::Unreachable, Some(community_id));
                self.state_manager
                    .clear_failure(FailureType::CredentialsInvalid, Some(community_id));
                locations
            }
            Err(e) => {
                self.handle_yardi_error(&e, community_id, &tracked.community.name)
                    .await;
                return Ok(());
            }
        };

        let yardi_residents = match self.yardi_client.fetch_residents(&credentials).await {
            Ok(residents) => residents,
            Err(e) => {
                self.handle_yardi_error(&e, community_id, &tracked.community.name)
                    .await;
                return Ok(());
            }
        };

        let yardi_residents = match self.resolve_resident_rooms(&yardi_residents, &yardi_locations)
        {
            Ok(residents) => residents,
            Err(e) => {
                self.handle_yardi_error(&e, community_id, &tracked.community.name)
                    .await;
                return Ok(());
            }
        };

        let yardi_rooms: Vec<YardiLocation> = yardi_locations
            .iter()
            .filter(|l| l.location_type == YardiLocationType::Room)
            .cloned()
            .collect();

        // Clear any data-related failures since we successfully parsed the response
        self.state_manager
            .clear_failure(FailureType::DataInvariantViolation, Some(community_id));
        self.state_manager
            .clear_failure(FailureType::UnexpectedResponse, Some(community_id));

        // Fetch resources-api data if dirty or refresh interval expired
        let (locations, residents) = if self
            .state_manager
            .needs_resources_refresh(community_id, resources_refresh_interval)
        {
            let locations = match self.resources_client.list_locations(community_id).await {
                Ok(locations) => locations,
                Err(ResourcesApiError::NotFound { reason }) => {
                    self.handle_not_found(community_id, reason.as_deref());
                    tracing::warn!(
                        community_id = %community_id,
                        reason = ?reason,
                        "Locations list not found"
                    );
                    return Ok(());
                }
                Err(e) => return Err(anyhow::anyhow!("Resources API error: {}", e)),
            };
            let residents = match self.resources_client.list_residents(community_id).await {
                Ok(residents) => residents,
                Err(ResourcesApiError::NotFound { reason }) => {
                    self.handle_not_found(community_id, reason.as_deref());
                    tracing::warn!(
                        community_id = %community_id,
                        reason = ?reason,
                        "Residents list not found"
                    );
                    return Ok(());
                }
                Err(e) => return Err(anyhow::anyhow!("Resources API error: {}", e)),
            };
            self.state_manager.mark_refreshed(community_id);
            (locations, residents)
        } else {
            // Use cached state
            (
                tracked.state.locations.clone(),
                tracked.state.residents.clone(),
            )
        };

        // Update local state
        let new_state = CommunityState {
            locations: locations.clone(),
            residents: residents.clone(),
        };
        self.state_manager.update_state(community_id, new_state);

        // Compute and apply differences in order to maintain referential integrity:
        // 1. Delete residents (they reference locations)
        // 2. Delete locations (no longer in Yardi)
        // 3. Create/update locations
        // 4. Create/update residents (they reference locations)

        let resident_delete_stats = self
            .delete_residents(community_id, &residents, &yardi_residents)
            .await?;
        let location_delete_stats = self
            .delete_locations(community_id, &locations, &yardi_rooms)
            .await?;
        let location_stats = self
            .sync_locations(community_id, &locations, &yardi_rooms)
            .await?;

        // Re-fetch locations after changes to get updated location list
        let updated_locations = match self.resources_client.list_locations(community_id).await {
            Ok(locations) => locations,
            Err(ResourcesApiError::NotFound { reason }) => {
                self.handle_not_found(community_id, reason.as_deref());
                tracing::warn!(
                    community_id = %community_id,
                    reason = ?reason,
                    "Locations list not found after updates"
                );
                return Ok(());
            }
            Err(e) => return Err(anyhow::anyhow!("Resources API error: {}", e)),
        };
        let resident_stats = self
            .sync_residents(
                community_id,
                &residents,
                &yardi_residents,
                &updated_locations,
            )
            .await?;

        let latest_residents = match self.resources_client.list_residents(community_id).await {
            Ok(residents) => residents,
            Err(ResourcesApiError::NotFound { reason }) => {
                self.handle_not_found(community_id, reason.as_deref());
                tracing::warn!(
                    community_id = %community_id,
                    reason = ?reason,
                    "Residents list not found after updates"
                );
                return Ok(());
            }
            Err(e) => return Err(anyhow::anyhow!("Resources API error: {}", e)),
        };

        let photo_stats = match self
            .sync_resident_photos(
                community_id,
                &latest_residents,
                &yardi_residents,
                &credentials,
            )
            .await
        {
            Ok(stats) => stats,
            Err(PhotoSyncError::Yardi(e)) => {
                self.handle_yardi_error(&e, community_id, &tracked.community.name)
                    .await;
                return Ok(());
            }
            Err(PhotoSyncError::Other(e)) => {
                return Err(e);
            }
        };

        let total_location_stats = SyncStats {
            created: location_stats.created,
            updated: location_stats.updated,
            deleted: location_delete_stats.deleted,
        };

        let total_resident_stats = SyncStats {
            created: resident_stats.created,
            updated: resident_stats.updated + photo_stats.updated,
            deleted: resident_delete_stats.deleted,
        };

        if total_location_stats.has_changes() || total_resident_stats.has_changes() {
            self.state_manager.mark_dirty(community_id);
            tracing::info!(
                community_id = %community_id,
                community_name = %tracked.community.name,
                locations_created = total_location_stats.created,
                locations_updated = total_location_stats.updated,
                locations_deleted = total_location_stats.deleted,
                residents_created = total_resident_stats.created,
                residents_updated = total_resident_stats.updated,
                residents_deleted = total_resident_stats.deleted,
                "Sync complete with changes"
            );
        } else {
            tracing::debug!(
                community_id = %community_id,
                "Sync complete, no changes"
            );
        }

        Ok(())
    }

    async fn handle_yardi_error(
        &self,
        error: &YardiError,
        community_id: Uuid,
        community_name: &str,
    ) {
        let notification =
            error.to_failure_notification(Some(community_id), Some(community_name.to_string()));

        tracing::warn!(
            community_id = %community_id,
            community_name = %community_name,
            failure_type = ?notification.failure_type,
            error = %error,
            "Community sync failed"
        );

        // Only publish if this is a new failure
        if self
            .state_manager
            .record_failure_notification(notification.clone())
        {
            tracing::info!(
                failure_type = ?notification.failure_type,
                community_id = ?notification.community_id,
                community_name = notification.community_name.as_deref().unwrap_or(""),
                hold_seconds = FAILURE_NOTIFICATION_HOLD.as_secs(),
                "Queued failure notification until hold period expires"
            );
        }
    }

    async fn delete_residents(
        &self,
        community_id: Uuid,
        current: &[Resident],
        yardi: &[YardiResident],
    ) -> Result<SyncStats> {
        let mut stats = SyncStats::default();

        let yardi_by_id: HashMap<&str, &YardiResident> =
            yardi.iter().map(|r| (r.id.as_str(), r)).collect();

        for resident in current {
            if let Some(ref_id) = &resident.yardi_reference_id
                && !yardi_by_id.contains_key(ref_id.as_str())
            {
                match self
                    .resources_client
                    .delete_resident(community_id, resident.id)
                    .await
                {
                    Ok(_) => {
                        stats.deleted += 1;
                        tracing::info!(
                            community_id = %community_id,
                            resident_id = %resident.id,
                            yardi_id = %ref_id,
                            "Deleted resident no longer in Yardi"
                        );
                    }
                    Err(ResourcesApiError::NotFound { reason }) => {
                        self.handle_not_found(community_id, reason.as_deref());
                        if reason.as_deref() == Some("resident_not_found") {
                            tracing::info!(
                                community_id = %community_id,
                                resident_id = %resident.id,
                                yardi_id = %ref_id,
                                "Resident already deleted while reconciling Yardi removal"
                            );
                            stats.deleted += 1;
                        } else {
                            tracing::warn!(
                                community_id = %community_id,
                                resident_id = %resident.id,
                                reason = ?reason,
                                "Resident delete returned not found"
                            );
                        }
                    }
                    Err(ResourcesApiError::Conflict { reason }) => {
                        self.handle_conflict(community_id, reason.as_deref());
                        tracing::warn!(
                            community_id = %community_id,
                            resident_id = %resident.id,
                            reason = ?reason,
                            "Resident delete conflict"
                        );
                    }
                    Err(e) => return Err(anyhow::anyhow!("Resources API error: {}", e)),
                }
            }
        }

        Ok(stats)
    }

    async fn delete_locations(
        &self,
        community_id: Uuid,
        current: &[Location],
        yardi: &[YardiLocation],
    ) -> Result<SyncStats> {
        let mut stats = SyncStats::default();

        let yardi_by_id: HashMap<&str, &YardiLocation> =
            yardi.iter().map(|l| (l.id.as_str(), l)).collect();

        for location in current {
            if let Some(ref_id) = &location.yardi_reference_id
                && !yardi_by_id.contains_key(ref_id.as_str())
            {
                match self
                    .resources_client
                    .delete_location(community_id, location.id)
                    .await
                {
                    Ok(_) => {
                        stats.deleted += 1;
                        tracing::info!(
                            community_id = %community_id,
                            location_id = %location.id,
                            yardi_id = %ref_id,
                            name = %location.name,
                            "Deleted location no longer in Yardi"
                        );
                    }
                    Err(ResourcesApiError::NotFound { reason }) => {
                        self.handle_not_found(community_id, reason.as_deref());
                        if reason.as_deref() == Some("location_not_found") {
                            tracing::info!(
                                community_id = %community_id,
                                location_id = %location.id,
                                yardi_id = %ref_id,
                                name = %location.name,
                                "Location already deleted while reconciling Yardi removal"
                            );
                            stats.deleted += 1;
                        } else {
                            tracing::warn!(
                                community_id = %community_id,
                                location_id = %location.id,
                                reason = ?reason,
                                "Location delete returned not found"
                            );
                        }
                    }
                    Err(ResourcesApiError::Conflict { reason }) => {
                        self.handle_conflict(community_id, reason.as_deref());
                        tracing::warn!(
                            community_id = %community_id,
                            location_id = %location.id,
                            reason = ?reason,
                            "Location delete conflict"
                        );
                    }
                    Err(e) => return Err(anyhow::anyhow!("Resources API error: {}", e)),
                }
            }
        }

        Ok(stats)
    }

    async fn sync_locations(
        &self,
        community_id: Uuid,
        current: &[Location],
        yardi: &[YardiLocation],
    ) -> Result<SyncStats> {
        let mut stats = SyncStats::default();

        let current_by_yardi_ref: HashMap<&str, &Location> = current
            .iter()
            .filter_map(|l| l.yardi_reference_id.as_deref().map(|ref_id| (ref_id, l)))
            .collect();

        let yardi_by_id: HashMap<&str, &YardiLocation> =
            yardi.iter().map(|l| (l.id.as_str(), l)).collect();

        for yardi_loc in yardi {
            if !current_by_yardi_ref.contains_key(yardi_loc.id.as_str()) {
                match self
                    .resources_client
                    .create_location(community_id, &yardi_loc.name, &yardi_loc.id)
                    .await
                {
                    Ok(_) => {
                        stats.created += 1;
                        tracing::info!(
                            community_id = %community_id,
                            yardi_id = %yardi_loc.id,
                            name = %yardi_loc.name,
                            "Created location from Yardi"
                        );
                    }
                    Err(ResourcesApiError::NotFound { reason }) => {
                        self.handle_not_found(community_id, reason.as_deref());
                        tracing::warn!(
                            community_id = %community_id,
                            yardi_id = %yardi_loc.id,
                            reason = ?reason,
                            "Location create not found"
                        );
                    }
                    Err(ResourcesApiError::Conflict { reason }) => {
                        self.handle_conflict(community_id, reason.as_deref());
                        tracing::warn!(
                            community_id = %community_id,
                            yardi_id = %yardi_loc.id,
                            reason = ?reason,
                            "Location create conflict"
                        );
                    }
                    Err(ResourcesApiError::BadRequest { reason }) => {
                        self.handle_bad_request(community_id, reason.as_deref());
                        tracing::warn!(
                            community_id = %community_id,
                            yardi_id = %yardi_loc.id,
                            reason = ?reason,
                            "Location create rejected"
                        );
                    }
                    Err(e) => return Err(anyhow::anyhow!("Resources API error: {}", e)),
                }
            }
        }

        for location in current {
            if let Some(ref_id) = &location.yardi_reference_id
                && let Some(yardi_loc) = yardi_by_id.get(ref_id.as_str())
                && location.name != yardi_loc.name
            {
                match self
                    .resources_client
                    .update_location(community_id, location.id, &yardi_loc.name, Some(ref_id))
                    .await
                {
                    Ok(_) => {
                        stats.updated += 1;
                        tracing::info!(
                            community_id = %community_id,
                            location_id = %location.id,
                            old_name = %location.name,
                            new_name = %yardi_loc.name,
                            "Updated location name from Yardi"
                        );
                    }
                    Err(ResourcesApiError::NotFound { reason }) => {
                        self.handle_not_found(community_id, reason.as_deref());
                        tracing::warn!(
                            community_id = %community_id,
                            location_id = %location.id,
                            reason = ?reason,
                            "Location update not found"
                        );
                    }
                    Err(ResourcesApiError::Conflict { reason }) => {
                        self.handle_conflict(community_id, reason.as_deref());
                        tracing::warn!(
                            community_id = %community_id,
                            location_id = %location.id,
                            reason = ?reason,
                            "Location update conflict"
                        );
                    }
                    Err(ResourcesApiError::BadRequest { reason }) => {
                        self.handle_bad_request(community_id, reason.as_deref());
                        tracing::warn!(
                            community_id = %community_id,
                            location_id = %location.id,
                            reason = ?reason,
                            "Location update rejected"
                        );
                    }
                    Err(e) => return Err(anyhow::anyhow!("Resources API error: {}", e)),
                }
            }
        }

        Ok(stats)
    }

    async fn sync_residents(
        &self,
        community_id: Uuid,
        current: &[Resident],
        yardi: &[YardiResident],
        locations: &[Location],
    ) -> Result<SyncStats> {
        let mut stats = SyncStats::default();

        let current_by_yardi_ref: HashMap<&str, &Resident> = current
            .iter()
            .filter_map(|r| r.yardi_reference_id.as_deref().map(|ref_id| (ref_id, r)))
            .collect();

        let yardi_by_id: HashMap<&str, &YardiResident> =
            yardi.iter().map(|r| (r.id.as_str(), r)).collect();

        let location_by_yardi_ref: HashMap<&str, &Location> = locations
            .iter()
            .filter_map(|l| l.yardi_reference_id.as_deref().map(|ref_id| (ref_id, l)))
            .collect();

        // Create new residents
        for yardi_res in yardi {
            if !current_by_yardi_ref.contains_key(yardi_res.id.as_str()) {
                let location = yardi_res
                    .room_id
                    .as_deref()
                    .and_then(|loc_id| location_by_yardi_ref.get(loc_id))
                    .copied();

                if let Some(location) = location {
                    match self
                        .resources_client
                        .create_resident(
                            community_id,
                            location.id,
                            &yardi_res.first_name,
                            &yardi_res.last_name,
                            &yardi_res.id,
                        )
                        .await
                    {
                        Ok(_) => {
                            stats.created += 1;
                            tracing::info!(
                                community_id = %community_id,
                                yardi_id = %yardi_res.id,
                                name = %yardi_res.full_name(),
                                location_id = %location.id,
                                "Created resident from Yardi"
                            );
                        }
                        Err(ResourcesApiError::NotFound { reason }) => {
                            self.handle_not_found(community_id, reason.as_deref());
                            tracing::warn!(
                                community_id = %community_id,
                                yardi_id = %yardi_res.id,
                                reason = ?reason,
                                "Resident create not found"
                            );
                        }
                        Err(ResourcesApiError::Conflict { reason }) => {
                            self.handle_conflict(community_id, reason.as_deref());
                            tracing::warn!(
                                community_id = %community_id,
                                yardi_id = %yardi_res.id,
                                reason = ?reason,
                                "Resident create conflict"
                            );
                        }
                        Err(ResourcesApiError::BadRequest { reason }) => {
                            self.handle_bad_request(community_id, reason.as_deref());
                            tracing::warn!(
                                community_id = %community_id,
                                yardi_id = %yardi_res.id,
                                reason = ?reason,
                                "Resident create rejected"
                            );
                        }
                        Err(e) => return Err(anyhow::anyhow!("Resources API error: {}", e)),
                    }
                } else {
                    tracing::warn!(
                        community_id = %community_id,
                        yardi_id = %yardi_res.id,
                        yardi_location_id = ?yardi_res.room_id,
                        "Cannot create resident: location not found in resources-api"
                    );
                    self.state_manager.mark_dirty(community_id);
                }
            }
        }

        // Update existing residents
        for resident in current {
            if let Some(ref_id) = &resident.yardi_reference_id
                && let Some(yardi_res) = yardi_by_id.get(ref_id.as_str())
            {
                let expected_location = yardi_res
                    .room_id
                    .as_deref()
                    .and_then(|loc_id| location_by_yardi_ref.get(loc_id))
                    .map(|l| l.id);

                let first_name_changed = resident.first_name != yardi_res.first_name;
                let last_name_changed = resident.last_name != yardi_res.last_name;
                let name_changed = first_name_changed || last_name_changed;
                let location_changed = expected_location
                    .map(|loc_id| loc_id != resident.location_id)
                    .unwrap_or(false);

                if (name_changed || location_changed)
                    && let Some(location_id) = expected_location
                {
                    match self
                        .resources_client
                        .update_resident(
                            community_id,
                            resident.id,
                            &yardi_res.first_name,
                            &yardi_res.last_name,
                            location_id,
                            Some(ref_id),
                        )
                        .await
                    {
                        Ok(_) => {
                            stats.updated += 1;
                            tracing::info!(
                                community_id = %community_id,
                                resident_id = %resident.id,
                                yardi_id = %ref_id,
                                name_changed = name_changed,
                                location_changed = location_changed,
                                "Updated resident from Yardi"
                            );
                        }
                        Err(ResourcesApiError::NotFound { reason }) => {
                            self.handle_not_found(community_id, reason.as_deref());
                            tracing::warn!(
                                community_id = %community_id,
                                resident_id = %resident.id,
                                reason = ?reason,
                                "Resident update not found"
                            );
                        }
                        Err(ResourcesApiError::Conflict { reason }) => {
                            self.handle_conflict(community_id, reason.as_deref());
                            tracing::warn!(
                                community_id = %community_id,
                                resident_id = %resident.id,
                                reason = ?reason,
                                "Resident update conflict"
                            );
                        }
                        Err(ResourcesApiError::BadRequest { reason }) => {
                            self.handle_bad_request(community_id, reason.as_deref());
                            tracing::warn!(
                                community_id = %community_id,
                                resident_id = %resident.id,
                                reason = ?reason,
                                "Resident update rejected"
                            );
                        }
                        Err(e) => return Err(anyhow::anyhow!("Resources API error: {}", e)),
                    }
                }
            }
        }

        Ok(stats)
    }

    async fn sync_resident_photos(
        &self,
        community_id: Uuid,
        current: &[Resident],
        yardi: &[YardiResident],
        credentials: &crate::models::YardiCredentials,
    ) -> std::result::Result<SyncStats, PhotoSyncError> {
        let mut stats = SyncStats::default();

        let current_by_yardi_ref: HashMap<&str, &Resident> = current
            .iter()
            .filter_map(|r| r.yardi_reference_id.as_deref().map(|ref_id| (ref_id, r)))
            .collect();

        for yardi_res in yardi {
            let Some(resident) = current_by_yardi_ref.get(yardi_res.id.as_str()).copied() else {
                continue;
            };

            let should_fetch = match yardi_res.last_updated.as_deref() {
                Some(last_updated) => {
                    self.state_manager
                        .patient_last_updated(community_id, yardi_res.id.as_str())
                        .as_deref()
                        != Some(last_updated)
                }
                None => true,
            };

            if !should_fetch {
                continue;
            }

            let yardi_photo = self
                .yardi_client
                .fetch_patient_photo(credentials, yardi_res.id.as_str())
                .await
                .map_err(PhotoSyncError::Yardi)?;

            if let Some(last_updated) = yardi_res.last_updated.as_deref() {
                self.state_manager.set_patient_last_updated(
                    community_id,
                    yardi_res.id.as_str(),
                    last_updated,
                );
            }

            match yardi_photo {
                Some(photo) => {
                    let bytes = base64::engine::general_purpose::STANDARD
                        .decode(photo.data_base64.as_bytes())
                        .map_err(|e| {
                            PhotoSyncError::Yardi(YardiError::InvalidData(format!(
                                "Yardi patient {} has invalid base64 photo data: {}",
                                yardi_res.id, e
                            )))
                        })?;

                    let digest = Sha256::digest(&bytes);
                    let mut digest_hex = String::with_capacity(digest.len() * 2);
                    for byte in digest {
                        use std::fmt::Write as _;
                        write!(&mut digest_hex, "{byte:02x}")
                            .expect("writing to String must succeed");
                    }
                    let hash = format!("sha256:{digest_hex}");
                    let unchanged = resident
                        .photo
                        .as_ref()
                        .is_some_and(|p| p.etag == hash && p.content_type == photo.content_type);
                    if unchanged {
                        continue;
                    }

                    match self
                        .resources_client
                        .put_resident_photo(community_id, resident.id, &photo.content_type, bytes)
                        .await
                    {
                        Ok(_) => {
                            stats.updated += 1;
                            tracing::info!(
                                community_id = %community_id,
                                resident_id = %resident.id,
                                yardi_id = %yardi_res.id,
                                content_type = %photo.content_type,
                                "Uploaded resident photo from Yardi"
                            );
                        }
                        Err(ResourcesApiError::NotFound { reason }) => {
                            self.handle_not_found(community_id, reason.as_deref());
                            tracing::warn!(
                                community_id = %community_id,
                                resident_id = %resident.id,
                                reason = ?reason,
                                "Resident photo upload not found"
                            );
                        }
                        Err(ResourcesApiError::Conflict { reason }) => {
                            self.handle_conflict(community_id, reason.as_deref());
                            tracing::warn!(
                                community_id = %community_id,
                                resident_id = %resident.id,
                                reason = ?reason,
                                "Resident photo upload conflict"
                            );
                        }
                        Err(ResourcesApiError::BadRequest { reason }) => {
                            self.handle_bad_request(community_id, reason.as_deref());
                            tracing::warn!(
                                community_id = %community_id,
                                resident_id = %resident.id,
                                reason = ?reason,
                                "Resident photo upload rejected"
                            );
                        }
                        Err(e) => {
                            return Err(PhotoSyncError::Other(anyhow::anyhow!(
                                "Resources API error: {}",
                                e
                            )));
                        }
                    }
                }
                None => {
                    if resident.photo.is_none() {
                        continue;
                    }
                    match self
                        .resources_client
                        .delete_resident_photo(community_id, resident.id)
                        .await
                    {
                        Ok(_) => {
                            stats.updated += 1;
                            tracing::info!(
                                community_id = %community_id,
                                resident_id = %resident.id,
                                yardi_id = %yardi_res.id,
                                "Deleted resident photo removed from Yardi"
                            );
                        }
                        Err(ResourcesApiError::NotFound { reason }) => {
                            self.handle_not_found(community_id, reason.as_deref());
                            tracing::warn!(
                                community_id = %community_id,
                                resident_id = %resident.id,
                                reason = ?reason,
                                "Resident photo delete not found"
                            );
                        }
                        Err(ResourcesApiError::Conflict { reason }) => {
                            self.handle_conflict(community_id, reason.as_deref());
                            tracing::warn!(
                                community_id = %community_id,
                                resident_id = %resident.id,
                                reason = ?reason,
                                "Resident photo delete conflict"
                            );
                        }
                        Err(ResourcesApiError::BadRequest { reason }) => {
                            self.handle_bad_request(community_id, reason.as_deref());
                            tracing::warn!(
                                community_id = %community_id,
                                resident_id = %resident.id,
                                reason = ?reason,
                                "Resident photo delete rejected"
                            );
                        }
                        Err(e) => {
                            return Err(PhotoSyncError::Other(anyhow::anyhow!(
                                "Resources API error: {}",
                                e
                            )));
                        }
                    }
                }
            }
        }

        Ok(stats)
    }

    fn handle_not_found(&self, community_id: Uuid, reason: Option<&str>) {
        match reason {
            Some("community_not_found") => self.state_manager.mark_community_list_dirty(),
            Some("location_not_found") | Some("resident_not_found") => {
                self.state_manager.mark_dirty(community_id);
            }
            _ => {}
        }
    }

    fn handle_conflict(&self, community_id: Uuid, reason: Option<&str>) {
        match reason {
            Some("yardi_integration_required") => self.state_manager.mark_community_list_dirty(),
            Some("yardi_reference_id_conflict")
            | Some("location_has_residents")
            | Some("resident_has_dependencies") => {
                self.state_manager.mark_dirty(community_id);
            }
            _ => {}
        }
    }

    fn handle_bad_request(&self, community_id: Uuid, reason: Option<&str>) {
        if matches!(reason, Some("location_not_found")) {
            self.state_manager.mark_dirty(community_id);
        }
    }

    fn resolve_resident_rooms(
        &self,
        residents: &[YardiResident],
        locations: &[YardiLocation],
    ) -> Result<Vec<YardiResident>, YardiError> {
        let location_map: HashMap<&str, &YardiLocation> =
            locations.iter().map(|l| (l.id.as_str(), l)).collect();
        let mut resolved = Vec::with_capacity(residents.len());

        for resident in residents {
            if resident.location_ids.is_empty() {
                return Err(YardiError::InvalidData(format!(
                    "Yardi patient {} has no location",
                    resident.id
                )));
            }

            let room_id = resident
                .location_ids
                .iter()
                .rev()
                .find_map(|location_id| {
                    location_map
                        .get(location_id.as_str())
                        .filter(|location| location.location_type == YardiLocationType::Room)
                        .map(|location| location.id.clone())
                })
                .ok_or_else(|| {
                    YardiError::InvalidData(format!(
                        "Yardi patient {} is not associated with a room",
                        resident.id
                    ))
                })?;

            let mut updated = resident.clone();
            updated.room_id = Some(room_id);
            resolved.push(updated);
        }

        Ok(resolved)
    }
}
