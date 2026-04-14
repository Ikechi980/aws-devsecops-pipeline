use std::sync::Arc;

use axum::{
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;

use crate::client::extract_client_id;
use crate::state::{AppState, ClientCommand};

pub async fn handler(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let client_id = match extract_client_id(&headers) {
        Ok(id) => id,
        Err(e) => {
            tracing::error!("Client identification failed: {}", e);
            return StatusCode::UNAUTHORIZED.into_response();
        }
    };

    tracing::info!("WebSocket upgrade request from client: {}", client_id);

    ws.on_upgrade(move |socket| handle_socket(socket, client_id, state))
}

async fn handle_socket(socket: WebSocket, client_id: String, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<ClientCommand>();

    let client_id_for_send = client_id.clone();
    let mut send_task = tokio::spawn(async move {
        while let Some(command) = rx.recv().await {
            let is_close = matches!(command, ClientCommand::Close);
            let result = match command {
                ClientCommand::Text(message) => sender.send(Message::Text(message.into())).await,
                ClientCommand::Close => {
                    tracing::info!(
                        "Closing WebSocket connection for client {}",
                        client_id_for_send
                    );
                    sender.send(Message::Close(None)).await
                }
            };

            if let Err(err) = result {
                tracing::debug!("Failed to send to client {}: {}", client_id_for_send, err);
                break;
            }

            if is_close {
                break;
            }
        }
    });

    if !state.register_client(client_id.clone(), tx.clone()) {
        tracing::info!(
            "Rejecting client {} because shutdown is in progress",
            client_id
        );
        let _ = tx.send(ClientCommand::Close);
        let _ = send_task.await;
        return;
    }

    tracing::info!("Client {} connected", client_id);

    let mut recv_task = tokio::spawn(async move {
        while let Some(result) = receiver.next().await {
            match result {
                Ok(Message::Close(_)) => break,
                Ok(Message::Ping(_)) => {}
                Ok(Message::Text(text)) => {
                    tracing::debug!("Received message from client: {:?}", text);
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::debug!("WebSocket error: {}", e);
                    break;
                }
            }
        }
    });

    tokio::select! {
        _ = &mut send_task => {
            recv_task.abort();
            let _ = recv_task.await;
        },
        _ = &mut recv_task => {
            send_task.abort();
            let _ = send_task.await;
        },
    }

    state.unregister_client(&client_id);
    tracing::info!("Client {} disconnected", client_id);
}
