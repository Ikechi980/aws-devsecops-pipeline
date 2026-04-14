use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use uuid::Uuid;

use crate::models::{Community, CommunityState, FailureNotification, FailureType};
use crate::timeouts::FAILURE_NOTIFICATION_HOLD;

/// Tracks state for all communities with Yardi integrations.
pub struct StateManager {
    communities: DashMap<Uuid, TrackedCommunity>,
    active_failures: DashMap<FailureKey, ()>,
    pending_notifications: DashMap<FailureKey, QueuedFailureNotification>,
    community_list_dirty: AtomicBool,
}

#[derive(Debug, Clone)]
pub struct TrackedCommunity {
    pub community: Community,
    pub state: CommunityState,
    pub dirty: bool,
    pub last_resources_refresh: Option<Instant>,
    pub patient_last_updated: HashMap<String, String>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct FailureKey {
    pub failure_type: FailureType,
    pub community_id: Option<Uuid>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct CommunitySyncStats {
    pub added: usize,
    pub removed: usize,
}

#[derive(Debug, Clone)]
struct QueuedFailureNotification {
    notification: FailureNotification,
    publish_after: Instant,
}

impl StateManager {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            communities: DashMap::new(),
            active_failures: DashMap::new(),
            pending_notifications: DashMap::new(),
            community_list_dirty: AtomicBool::new(true),
        })
    }

    /// Returns all community IDs currently being tracked.
    pub fn tracked_community_ids(&self) -> Vec<Uuid> {
        self.communities.iter().map(|r| *r.key()).collect()
    }

    /// Returns a copy of a tracked community if it exists.
    pub fn get_community(&self, id: Uuid) -> Option<TrackedCommunity> {
        self.communities.get(&id).map(|r| r.clone())
    }

    /// Adds or updates a community in the tracked set.
    pub fn upsert_community(&self, community: Community) {
        self.communities
            .entry(community.id)
            .and_modify(|tc| {
                tc.community = community.clone();
            })
            .or_insert_with(|| TrackedCommunity {
                community,
                state: CommunityState::default(),
                dirty: true,
                last_resources_refresh: None,
                patient_last_updated: HashMap::new(),
            });
    }

    /// Removes a community from tracking.
    pub fn remove_community(&self, id: Uuid) {
        self.communities.remove(&id);

        // Clean up any community-specific failure tracking
        let failures_to_remove: Vec<FailureKey> = self
            .active_failures
            .iter()
            .filter(|r| r.key().community_id == Some(id))
            .map(|r| r.key().clone())
            .collect();

        for key in failures_to_remove {
            self.active_failures.remove(&key);
            self.pending_notifications.remove(&key);
        }
    }

    /// Updates the state for a community.
    pub fn update_state(&self, community_id: Uuid, state: CommunityState) {
        if let Some(mut tc) = self.communities.get_mut(&community_id) {
            tc.state = state;
        }
    }

    pub fn patient_last_updated(&self, community_id: Uuid, patient_id: &str) -> Option<String> {
        self.communities
            .get(&community_id)
            .and_then(|tc| tc.patient_last_updated.get(patient_id).cloned())
    }

    pub fn set_patient_last_updated(
        &self,
        community_id: Uuid,
        patient_id: &str,
        last_updated: &str,
    ) {
        if let Some(mut tc) = self.communities.get_mut(&community_id) {
            tc.patient_last_updated
                .insert(patient_id.to_string(), last_updated.to_string());
        }
    }

    /// Marks a community as dirty, indicating its resources-api state should be refreshed.
    pub fn mark_dirty(&self, community_id: Uuid) {
        if let Some(mut tc) = self.communities.get_mut(&community_id) {
            tc.dirty = true;
        }
    }

    /// Marks a community as refreshed and clears the dirty flag.
    pub fn mark_refreshed(&self, community_id: Uuid) {
        if let Some(mut tc) = self.communities.get_mut(&community_id) {
            tc.dirty = false;
            tc.last_resources_refresh = Some(Instant::now());
        }
    }

    /// Marks the community list as dirty, indicating it should be refreshed.
    pub fn mark_community_list_dirty(&self) {
        self.community_list_dirty.store(true, Ordering::Release);
    }

    /// Marks the community list as refreshed and clears the dirty flag.
    pub fn mark_community_list_refreshed(&self) {
        self.community_list_dirty.store(false, Ordering::Release);
    }

    /// Returns true if the community list should be refreshed.
    pub fn is_community_list_dirty(&self) -> bool {
        self.community_list_dirty.load(Ordering::Acquire)
    }

    /// Checks if a community needs a resources-api refresh.
    pub fn needs_resources_refresh(&self, community_id: Uuid, refresh_interval: Duration) -> bool {
        if let Some(tc) = self.communities.get(&community_id) {
            if tc.dirty {
                return true;
            }

            if let Some(last_refresh) = tc.last_resources_refresh {
                last_refresh.elapsed() >= refresh_interval
            } else {
                // Never refreshed
                true
            }
        } else {
            false
        }
    }

    /// Synchronizes the tracked communities with a list from resources-api.
    /// Adds new communities with Yardi integrations, updates existing ones,
    /// and removes ones that no longer have Yardi integrations.
    pub fn sync_communities(&self, communities: Vec<Community>) -> CommunitySyncStats {
        let mut stats = CommunitySyncStats::default();

        let incoming_ids: HashSet<Uuid> = communities
            .iter()
            .filter(|c| c.has_yardi_integration())
            .map(|c| c.id)
            .collect();

        // Remove communities that are no longer tracked
        let to_remove: Vec<Uuid> = self
            .communities
            .iter()
            .filter(|r| !incoming_ids.contains(r.key()))
            .map(|r| *r.key())
            .collect();

        for id in to_remove {
            self.remove_community(id);
            stats.removed += 1;
            tracing::info!(community_id = %id, "Removed community from tracking");
        }

        // Add or update communities with Yardi integrations
        for community in communities {
            if community.has_yardi_integration() {
                let is_new = !self.communities.contains_key(&community.id);
                self.upsert_community(community.clone());
                if is_new {
                    stats.added += 1;
                    tracing::info!(
                        community_id = %community.id,
                        community_name = %community.name,
                        "Added community to tracking"
                    );
                }
            }
        }

        stats
    }

    /// Records a failure notification and queues it for publishing if new.
    pub fn record_failure_notification(&self, notification: FailureNotification) -> bool {
        self.record_failure_notification_at(notification, Instant::now(), FAILURE_NOTIFICATION_HOLD)
    }

    fn record_failure_notification_at(
        &self,
        notification: FailureNotification,
        now: Instant,
        hold_duration: Duration,
    ) -> bool {
        let key = FailureKey {
            failure_type: notification.failure_type,
            community_id: notification.community_id,
        };

        let is_new = self.active_failures.insert(key.clone(), ()).is_none();
        if is_new {
            self.pending_notifications.insert(
                key,
                QueuedFailureNotification {
                    notification,
                    publish_after: now + hold_duration,
                },
            );
        }

        is_new
    }

    #[cfg(test)]
    fn pending_failure_notifications(&self) -> Vec<(FailureKey, FailureNotification)> {
        self.pending_notifications
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().notification.clone()))
            .collect()
    }

    pub fn ready_failure_notifications(&self) -> Vec<(FailureKey, FailureNotification)> {
        self.ready_failure_notifications_at(Instant::now())
    }

    fn ready_failure_notifications_at(
        &self,
        now: Instant,
    ) -> Vec<(FailureKey, FailureNotification)> {
        self.pending_notifications
            .iter()
            .filter(|entry| entry.value().publish_after <= now)
            .map(|entry| (entry.key().clone(), entry.value().notification.clone()))
            .collect()
    }

    pub fn clear_pending_failure(&self, key: &FailureKey) {
        self.pending_notifications.remove(key);
    }

    pub fn is_failure_active(&self, key: &FailureKey) -> bool {
        self.active_failures.contains_key(key)
    }

    /// Clears a recorded failure, indicating it has been resolved.
    pub fn clear_failure(&self, failure_type: FailureType, community_id: Option<Uuid>) {
        let key = FailureKey {
            failure_type,
            community_id,
        };
        self.active_failures.remove(&key);
        self.pending_notifications.remove(&key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notification(failure_type: FailureType, community_id: Option<Uuid>) -> FailureNotification {
        FailureNotification {
            failure_type,
            community_id,
            community_name: None,
            message: "test".to_string(),
            details: None,
            timestamp: "now".to_string(),
        }
    }

    #[test]
    fn record_failure_notification_deduplicates() {
        let state = StateManager::new();
        let community_id = Some(Uuid::new_v4());

        let first =
            state.record_failure_notification(notification(FailureType::Unreachable, community_id));
        let second =
            state.record_failure_notification(notification(FailureType::Unreachable, community_id));

        assert!(first, "first failure should be recorded");
        assert!(!second, "duplicate failure should be deduplicated");

        let pending = state.pending_failure_notifications();
        assert_eq!(
            pending.len(),
            1,
            "pending notifications should be deduplicated"
        );
    }

    #[test]
    fn failure_notification_is_not_ready_before_hold_expires() {
        let state = StateManager::new();
        let community_id = Some(Uuid::new_v4());
        let now = Instant::now();

        state.record_failure_notification_at(
            notification(FailureType::Unreachable, community_id),
            now,
            Duration::from_secs(300),
        );

        assert!(state.ready_failure_notifications_at(now).is_empty());
        assert!(
            state
                .ready_failure_notifications_at(now + Duration::from_secs(299))
                .is_empty()
        );
    }

    #[test]
    fn failure_notification_is_ready_after_hold_expires() {
        let state = StateManager::new();
        let community_id = Some(Uuid::new_v4());
        let now = Instant::now();

        state.record_failure_notification_at(
            notification(FailureType::Unreachable, community_id),
            now,
            Duration::from_secs(300),
        );

        let ready = state.ready_failure_notifications_at(now + Duration::from_secs(300));
        assert_eq!(
            ready.len(),
            1,
            "notification should become ready after hold"
        );
    }

    #[test]
    fn duplicate_failure_does_not_extend_publish_deadline() {
        let state = StateManager::new();
        let community_id = Some(Uuid::new_v4());
        let now = Instant::now();

        state.record_failure_notification_at(
            notification(FailureType::UnexpectedResponse, community_id),
            now,
            Duration::from_secs(300),
        );
        state.record_failure_notification_at(
            notification(FailureType::UnexpectedResponse, community_id),
            now + Duration::from_secs(120),
            Duration::from_secs(300),
        );

        assert!(
            state
                .ready_failure_notifications_at(now + Duration::from_secs(299))
                .is_empty(),
            "duplicate detection should not make notification ready early"
        );
        let ready = state.ready_failure_notifications_at(now + Duration::from_secs(300));
        assert_eq!(
            ready.len(),
            1,
            "duplicate detection should keep original publish deadline"
        );
        assert_eq!(
            state
                .ready_failure_notifications_at(now + Duration::from_secs(419))
                .len(),
            1,
            "duplicate detection should not delay readiness"
        );
    }

    #[test]
    fn clear_failure_removes_pending_and_active() {
        let state = StateManager::new();
        let community_id = Some(Uuid::new_v4());

        state.record_failure_notification(notification(
            FailureType::UnexpectedResponse,
            community_id,
        ));
        assert!(state.is_failure_active(&FailureKey {
            failure_type: FailureType::UnexpectedResponse,
            community_id,
        }));

        state.clear_failure(FailureType::UnexpectedResponse, community_id);
        assert!(!state.is_failure_active(&FailureKey {
            failure_type: FailureType::UnexpectedResponse,
            community_id,
        }));
        assert!(state.pending_failure_notifications().is_empty());
    }

    #[test]
    fn cleared_failure_before_hold_expires_never_becomes_ready() {
        let state = StateManager::new();
        let community_id = Some(Uuid::new_v4());
        let now = Instant::now();

        state.record_failure_notification_at(
            notification(FailureType::CredentialsInvalid, community_id),
            now,
            Duration::from_secs(300),
        );
        state.clear_failure(FailureType::CredentialsInvalid, community_id);

        assert!(
            state
                .ready_failure_notifications_at(now + Duration::from_secs(600))
                .is_empty()
        );
    }

    #[test]
    fn ready_failure_stays_queued_until_cleared() {
        let state = StateManager::new();
        let community_id = Some(Uuid::new_v4());
        let now = Instant::now();

        state.record_failure_notification_at(
            notification(FailureType::DataInvariantViolation, community_id),
            now,
            Duration::from_secs(300),
        );

        let ready_at_hold = state.ready_failure_notifications_at(now + Duration::from_secs(300));
        let ready_later = state.ready_failure_notifications_at(now + Duration::from_secs(360));

        assert_eq!(ready_at_hold.len(), 1);
        assert_eq!(
            ready_later.len(),
            1,
            "ready notifications should remain queued until publish succeeds or failure clears"
        );
        assert_eq!(state.pending_failure_notifications().len(), 1);
    }

    #[test]
    fn remove_community_clears_failures() {
        let state = StateManager::new();
        let community_id = Uuid::new_v4();

        state.record_failure_notification(notification(
            FailureType::CredentialsInvalid,
            Some(community_id),
        ));

        state.remove_community(community_id);

        assert!(!state.is_failure_active(&FailureKey {
            failure_type: FailureType::CredentialsInvalid,
            community_id: Some(community_id),
        }));
        assert!(state.pending_failure_notifications().is_empty());
    }
}
