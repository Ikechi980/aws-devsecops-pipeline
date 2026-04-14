use crate::{core_resources::CoreResourcesClient, events_repo::EventsRepo, systems::SystemsClient};
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub systems: SystemsClient,
    pub core_resources: CoreResourcesClient,
    pub events_repo: Arc<dyn EventsRepo>,
    pub events_limit_default: u32,
    pub events_limit_max: u32,
    pub allow_unauthenticated: bool,
}
