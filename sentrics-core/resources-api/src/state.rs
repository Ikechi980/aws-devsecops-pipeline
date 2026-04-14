use crate::events::EventPublisher;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub publisher: Arc<EventPublisher>,
}
