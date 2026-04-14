use std::sync::atomic::{AtomicBool, Ordering};

use dashmap::DashMap;
use tokio::sync::mpsc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientCommand {
    Text(String),
    Close,
}

pub type ClientTx = mpsc::UnboundedSender<ClientCommand>;

pub struct AppState {
    /// Maps normalized community_id to their message sender channel.
    active_clients: DashMap<String, ClientTx>,
    shutting_down: AtomicBool,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            active_clients: DashMap::new(),
            shutting_down: AtomicBool::new(false),
        }
    }

    pub fn register_client(&self, client_id: String, tx: ClientTx) -> bool {
        if self.is_shutting_down() {
            return false;
        }

        self.active_clients.insert(client_id.clone(), tx);

        if self.is_shutting_down() {
            if let Some((_, tx)) = self.active_clients.remove(&client_id) {
                let _ = tx.send(ClientCommand::Close);
            }
            return false;
        }

        true
    }

    pub fn unregister_client(&self, client_id: &str) {
        self.active_clients.remove(client_id);
    }

    pub fn send_to_client(&self, client_id: &str, message: String) -> bool {
        if self.is_shutting_down() {
            return false;
        }

        if let Some(tx) = self.active_clients.get(client_id) {
            tx.send(ClientCommand::Text(message)).is_ok()
        } else {
            false
        }
    }

    pub fn begin_shutdown(&self) {
        if self.shutting_down.swap(true, Ordering::SeqCst) {
            return;
        }

        let client_count = self.active_clients.len();
        tracing::info!(
            client_count,
            "Closing active WebSocket clients for shutdown"
        );

        for entry in self.active_clients.iter() {
            let _ = entry.value().send(ClientCommand::Close);
        }
    }

    pub fn is_shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::SeqCst)
    }

    pub fn client_count(&self) -> usize {
        self.active_clients.len()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{AppState, ClientCommand};
    use tokio::sync::mpsc;

    #[test]
    fn begin_shutdown_notifies_registered_clients() {
        let state = AppState::new();
        let (tx, mut rx) = mpsc::unbounded_channel();

        assert!(state.register_client("alpha".to_string(), tx));

        state.begin_shutdown();

        assert!(state.is_shutting_down());
        assert_eq!(rx.try_recv(), Ok(ClientCommand::Close));
    }

    #[test]
    fn register_client_rejects_when_shutdown_has_started() {
        let state = AppState::new();
        let (tx, _rx) = mpsc::unbounded_channel();

        state.begin_shutdown();

        assert!(!state.register_client("alpha".to_string(), tx));
        assert_eq!(state.client_count(), 0);
    }

    #[test]
    fn send_to_client_routes_text_before_shutdown() {
        let state = AppState::new();
        let (tx, mut rx) = mpsc::unbounded_channel();

        assert!(state.register_client("alpha".to_string(), tx));
        assert!(state.send_to_client("alpha", "hello".to_string()));
        assert_eq!(rx.try_recv(), Ok(ClientCommand::Text("hello".to_string())));
    }
}
