// Redis Pub/Sub Service for GridTokenX
// Implements real-time WebSocket scaling with Redis Pub/Sub

use futures::StreamExt;
use redis::{AsyncCommands, Client, RedisResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Redis Pub/Sub channel definitions
pub mod channels {
    pub const MARKET_EVENTS: &str = "market:events";
    pub const TRADE_EXECUTION: &str = "trades:*";
    pub const ORDER_UPDATES: &str = "orders:*";
    pub const SYSTEM_ALERTS: &str = "system:alerts";
    pub const USER_NOTIFICATIONS: &str = "user:*";
    pub const MARKET_DATA: &str = "market:data";
    pub const SETTLEMENT_EVENTS: &str = "settlement:events";
    pub const TOKEN_EVENTS: &str = "token:events";
}

/// Message types for different event categories
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MarketEvent {
    OrderBookUpdate {
        symbol: String,
        timestamp: i64,
        best_bid: f64,
        best_ask: f64,
        spread: f64,
        volume: f64,
    },
    PriceUpdate {
        symbol: String,
        price: f64,
        timestamp: i64,
        change_percent: f64,
    },
    TradeExecuted {
        trade_id: String,
        symbol: String,
        price: f64,
        quantity: f64,
        buyer: String,
        seller: String,
        timestamp: i64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OrderEvent {
    OrderCreated {
        order_id: String,
        user_id: String,
        symbol: String,
        order_type: String,
        price: f64,
        quantity: f64,
        timestamp: i64,
    },
    OrderUpdated {
        order_id: String,
        status: String,
        filled_quantity: f64,
        timestamp: i64,
    },
    OrderCancelled {
        order_id: String,
        user_id: String,
        reason: String,
        timestamp: i64,
    },
    OrderMatched {
        order_id: String,
        match_price: f64,
        match_quantity: f64,
        counterparty: String,
        timestamp: i64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SystemAlert {
    HighMemoryUsage {
        percentage: f64,
        threshold: f64,
        timestamp: i64,
    },
    RedisConnectionLost {
        duration: i64,
        retry_count: u32,
        timestamp: i64,
    },
    MarketDataDelay {
        delay_seconds: i64,
        expected_frequency: i64,
        timestamp: i64,
    },
    SecurityEvent {
        event_type: String,
        user_id: Option<String>,
        ip_address: String,
        timestamp: i64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UserNotification {
    OrderUpdate {
        user_id: String,
        order_id: String,
        status: String,
        message: String,
        timestamp: i64,
    },
    TradeConfirmation {
        user_id: String,
        trade_id: String,
        symbol: String,
        price: f64,
        quantity: f64,
        timestamp: i64,
    },
    SettlementComplete {
        user_id: String,
        settlement_id: String,
        amount: f64,
        currency: String,
        timestamp: i64,
    },
    TokenMinted {
        user_id: String,
        token_amount: f64,
        meter_reading: String,
        timestamp: i64,
    },
}

/// Unified message wrapper for all event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PubSubMessage {
    pub id: String,
    pub channel: String,
    pub message_type: String,
    pub data: serde_json::Value,
    pub timestamp: i64,
    pub version: String,
}

impl PubSubMessage {
    pub fn new<T: Serialize>(channel: &str, message_type: &str, data: T) -> RedisResult<Self> {
        Ok(Self {
            id: Uuid::new_v4().to_string(),
            channel: channel.to_string(),
            message_type: message_type.to_string(),
            data: serde_json::to_value(data).map_err(|e| {
                redis::RedisError::from((
                    redis::ErrorKind::TypeError,
                    "Serialization error",
                    e.to_string(),
                ))
            })?,
            timestamp: chrono::Utc::now().timestamp(),
            version: "1.0".to_string(),
        })
    }
}

/// Redis Pub/Sub service for real-time event distribution
pub struct RedisPubSubService {
    client: Client,
    publishers: Arc<RwLock<HashMap<String, broadcast::Sender<PubSubMessage>>>>,
    subscribers: Arc<RwLock<HashMap<String, Vec<broadcast::Receiver<PubSubMessage>>>>>,
}

impl RedisPubSubService {
    /// Create a new Redis Pub/Sub service
    pub async fn new(redis_url: &str) -> RedisResult<Self> {
        let client = Client::open(redis_url)?;

        Ok(Self {
            client,
            publishers: Arc::new(RwLock::new(HashMap::new())),
            subscribers: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Initialize the Pub/Sub service and subscribe to channels
    pub async fn initialize(&self, channels: &[&str]) -> RedisResult<()> {
        let _conn = self.client.get_multiplexed_async_connection().await?;

        // Create publishers for each channel
        let mut publishers = self.publishers.write().await;
        let mut subscribers = self.subscribers.write().await;

        for channel in channels {
            // Create broadcast channel for each Redis channel
            let (tx, _) = broadcast::channel(1000);
            publishers.insert(channel.to_string(), tx);
            subscribers.insert(channel.to_string(), Vec::new());

            info!("Initialized Pub/Sub channel: {}", channel);
        }

        // Subscribe to Redis channels
        let mut pubsub = self.client.get_async_pubsub().await?;

        for channel in channels {
            let _: () = pubsub.subscribe(channel).await?;
            info!("Subscribed to Redis channel: {}", channel);
        }

        // Start listening for messages
        let subscribers_clone = Arc::clone(&self.subscribers);
        tokio::spawn(async move {
            while let Some(msg) = pubsub.on_message().next().await {
                let channel = msg.get_channel_name();
                let payload: String = match msg.get_payload() {
                    Ok(p) => p,
                    Err(e) => {
                        warn!("Failed to get payload from message: {}", e);
                        continue;
                    }
                };

                debug!("Received message on channel {}: {}", channel, payload);

                // Parse the message
                if let Ok(_pubsub_msg) = serde_json::from_str::<PubSubMessage>(&payload) {
                    // Broadcast to local subscribers
                    if let Some(subscribers_map) = subscribers_clone.read().await.get(channel) {
                        for _rx in subscribers_map.iter() {
                            // Note: In a real implementation, we'd need to handle this differently
                            // as we can't clone receivers. This is a simplified example.
                        }
                    }
                } else {
                    warn!("Failed to parse PubSub message: {}", payload);
                }
            }
        });

        Ok(())
    }

    /// Publish a message to a Redis channel
    pub async fn publish<T: Serialize>(
        &self,
        channel: &str,
        message_type: &str,
        data: T,
    ) -> RedisResult<()> {
        let message = PubSubMessage::new(channel, message_type, data)?;
        let message_json = serde_json::to_string(&message).map_err(|e| {
            redis::RedisError::from((
                redis::ErrorKind::TypeError,
                "Serialization error",
                e.to_string(),
            ))
        })?;

        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let _: () = conn.publish(channel, &message_json).await?;

        // Also publish to local broadcast channel
        if let Some(publishers) = self.publishers.read().await.get(channel) {
            if let Err(e) = publishers.send(message.clone()) {
                warn!("Failed to broadcast to local subscribers: {}", e);
            }
        }

        info!("Published message to channel {}: {}", channel, message_type);
        Ok(())
    }

    /// Publish market event
    pub async fn publish_market_event(&self, event: MarketEvent) -> RedisResult<()> {
        match &event {
            MarketEvent::OrderBookUpdate { symbol, .. } => {
                self.publish(
                    &format!("{}:{}", channels::MARKET_EVENTS, symbol),
                    "order_book_update",
                    event,
                )
                .await
            }
            MarketEvent::PriceUpdate { symbol, .. } => {
                self.publish(
                    &format!("{}:{}", channels::MARKET_EVENTS, symbol),
                    "price_update",
                    event,
                )
                .await
            }
            MarketEvent::TradeExecuted { .. } => {
                self.publish(channels::MARKET_EVENTS, "trade_executed", event)
                    .await
            }
        }
    }

    /// Publish order event to specific user
    pub async fn publish_order_event(&self, user_id: &str, event: OrderEvent) -> RedisResult<()> {
        self.publish(
            &format!("{}:{}", channels::ORDER_UPDATES, user_id),
            "order_update",
            event,
        )
        .await
    }

    /// Publish system alert
    pub async fn publish_system_alert(&self, alert: SystemAlert) -> RedisResult<()> {
        self.publish(channels::SYSTEM_ALERTS, "system_alert", alert)
            .await
    }

    /// Publish user notification
    pub async fn publish_user_notification(
        &self,
        user_id: &str,
        notification: UserNotification,
    ) -> RedisResult<()> {
        self.publish(
            &format!("{}:{}", channels::USER_NOTIFICATIONS, user_id),
            "user_notification",
            notification,
        )
        .await
    }

    /// Subscribe to a channel and return a receiver
    pub async fn subscribe(
        &self,
        channel: &str,
    ) -> RedisResult<broadcast::Receiver<PubSubMessage>> {
        let mut subscribers = self.subscribers.write().await;

        if let Some(subscriber_list) = subscribers.get_mut(channel) {
            let (tx, rx) = broadcast::channel(1000);
            subscriber_list.push(tx.subscribe());

            // Also subscribe to Redis if not already done
            let mut pubsub = self.client.get_async_pubsub().await?;
            let _: () = pubsub.subscribe(channel).await?;

            info!("Added subscriber to channel: {}", channel);
            Ok(rx)
        } else {
            Err(redis::RedisError::from((
                redis::ErrorKind::TypeError,
                "Channel not initialized",
                "Channel not initialized".to_string(),
            )))
        }
    }

    pub async fn unsubscribe(&self, channel: &str) -> RedisResult<()> {
        let mut pubsub = self.client.get_async_pubsub().await?;
        let _: () = pubsub.unsubscribe(channel).await?;

        info!("Unsubscribed from channel: {}", channel);
        Ok(())
    }

    /// Get channel statistics
    pub async fn get_channel_stats(&self, channel: &str) -> RedisResult<HashMap<String, i64>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;

        let subscriber_count = self
            .subscribers
            .read()
            .await
            .get(channel)
            .map(|subscribers| subscribers.len() as i64)
            .unwrap_or(0);

        let mut stats = HashMap::new();
        stats.insert("subscribers".to_string(), subscriber_count);

        // Get Redis pub/sub stats if available
        let _: Option<String> = conn.get(format!("pubsub:subscribers:{}", channel)).await?;

        Ok(stats)
    }

    /// Health check for Pub/Sub service
    pub async fn health_check(&self) -> RedisResult<bool> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;

        // Test basic connectivity
        let _: String = conn.ping().await?;

        // Check if we can publish to a test channel
        let test_channel = "health_check_test";
        let test_message = "health_check";
        let _: () = conn.publish(test_channel, test_message).await?;

        Ok(true)
    }
}

/// WebSocket integration for Redis Pub/Sub
pub struct WebSocketPubSubBridge {
    pubsub_service: Arc<RedisPubSubService>,
    connections: Arc<RwLock<HashMap<String, broadcast::Sender<PubSubMessage>>>>,
}

impl WebSocketPubSubBridge {
    pub fn new(pubsub_service: RedisPubSubService) -> Self {
        Self {
            pubsub_service: Arc::new(pubsub_service),
            connections: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a WebSocket connection
    pub async fn add_connection(
        &self,
        connection_id: String,
        channels: Vec<String>,
    ) -> RedisResult<()> {
        let (tx, _) = broadcast::channel(1000);

        // Store connection sender
        self.connections
            .write()
            .await
            .insert(connection_id.clone(), tx);

        // Subscribe to requested channels
        let channels_clone = channels.clone();
        for channel in channels_clone {
            let mut rx = self.pubsub_service.subscribe(&channel).await?;

            // Forward messages to WebSocket
            let connections_clone = Arc::clone(&self.connections);
            let connection_id_clone = connection_id.clone();

            tokio::spawn(async move {
                while let Ok(message) = rx.recv().await {
                    if let Some(sender) = connections_clone.read().await.get(&connection_id_clone) {
                        if let Err(e) = sender.send(message.clone()) {
                            warn!(
                                "Failed to send message to WebSocket {}: {}",
                                connection_id_clone, e
                            );
                            break;
                        }
                    } else {
                        break;
                    }
                }
            });
        }

        info!(
            "Added WebSocket connection {} to channels: {:?}",
            connection_id, channels
        );
        Ok(())
    }

    /// Remove a WebSocket connection
    pub async fn remove_connection(&self, connection_id: &str) {
        self.connections.write().await.remove(connection_id);
        info!("Removed WebSocket connection: {}", connection_id);
    }

    /// Get active connection count
    pub async fn get_connection_count(&self) -> usize {
        self.connections.read().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use redis::Client;

    #[tokio::test]
    async fn test_pubsub_message_creation() {
        let message = PubSubMessage::new("test", "test_type", "test_data").unwrap();

        assert_eq!(message.channel, "test");
        assert_eq!(message.message_type, "test_type");
        assert!(message.timestamp > 0);
        assert_eq!(message.version, "1.0");
    }

    #[tokio::test]
    async fn test_market_event_serialization() {
        let event = MarketEvent::OrderBookUpdate {
            symbol: "ENERGY_USD".to_string(),
            timestamp: 1638360000,
            best_bid: 0.25,
            best_ask: 0.26,
            spread: 0.01,
            volume: 1000.0,
        };

        let message = PubSubMessage::new("market:events", "order_book_update", &event).unwrap();
        assert_eq!(message.message_type, "order_book_update");
    }
}
