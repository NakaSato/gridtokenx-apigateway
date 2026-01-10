use rustc_hash::FxHashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use super::types::WsMessage;

/// WebSocket connection manager
#[derive(Debug, Clone)]
pub struct ConnectionManager {
    /// Active connections by user
    connections: Arc<RwLock<FxHashMap<Uuid, broadcast::Sender<WsMessage>>>>,
    /// Global message broadcaster
    broadcaster: broadcast::Sender<WsMessage>,
}

impl ConnectionManager {
    pub fn new() -> Self {
        let (broadcaster, _) = broadcast::channel(1000);
        Self {
            connections: Arc::new(RwLock::new(FxHashMap::default())),
            broadcaster,
        }
    }

    /// Add a new connection
    pub async fn add_connection(&self, user_id: Uuid) -> broadcast::Receiver<WsMessage> {
        let (tx, rx) = broadcast::channel(100);
        let mut connections = self.connections.write().await;
        connections.insert(user_id, tx);
        rx
    }

    /// Remove a connection
    pub async fn remove_connection(&self, user_id: &Uuid) {
        let mut connections = self.connections.write().await;
        connections.remove(user_id);
    }

    /// Send message to specific user
    pub async fn send_to_user(
        &self,
        user_id: Uuid,
        message: WsMessage,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let connections = self.connections.read().await;
        if let Some(sender) = connections.get(&user_id) {
            sender.send(message)?;
        }
        Ok(())
    }

    /// Broadcast message to all connections
    pub async fn broadcast(
        &self,
        message: WsMessage,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let _ = self.broadcaster.send(message);
        Ok(())
    }

    /// Get number of active connections
    pub async fn connection_count(&self) -> usize {
        let connections = self.connections.read().await;
        connections.len()
    }
}
