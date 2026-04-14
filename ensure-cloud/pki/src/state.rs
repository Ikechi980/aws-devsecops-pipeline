use std::sync::Arc;

use crate::ca_client::CaClient;
use crate::community::CommunityLookup;

#[derive(Clone)]
pub struct AppState {
    pub lookup: Arc<dyn CommunityLookup>,
    pub ca_client: Arc<dyn CaClient>,
}
