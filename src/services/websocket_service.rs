use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt, stream::SplitSink};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};
use tracing::{error, info, warn};
use uuid::Uuid;

/// WebSocket message types for real-time market updates
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MarketEvent {
    /// New offer created in the market
    OfferCreated {
        offer_id: String,
        energy_amount: f64,
        price_per_kwh: f64,
        energy_source: String,
        location: String,
        created_by: String,
    },
    /// Offer updated (e.g., status changed)
    OfferUpdated {
        offer_id: String,
        status: String,
        energy_amount: Option<f64>,
    },
    /// New order placed
    OrderCreated {
        order_id: String,
        energy_amount: f64,
        max_price_per_kwh: f64,
        energy_source: Option<String>,
        created_by: String,
    },
    /// Order matched with an offer
    OrderMatched {
        order_id: String,
        offer_id: String,
        transaction_id: String,
        matched_amount: f64,
        price_per_kwh: f64,
    },
    /// Transaction status changed
    TransactionUpdated {
        transaction_id: String,
        status: String,
        buyer_id: String,
        seller_id: String,
    },
    /// Market statistics update
    MarketStats {
        total_active_offers: i64,
        total_pending_orders: i64,
        average_price: f64,
        total_volume_24h: f64,
    },
    /// Order book update (buy side)
    OrderBookBuyUpdate {
        price_levels: Vec<PriceLevel>,
        best_bid: Option<String>,
        timestamp: String,
    },
    /// Order book update (sell side)
    OrderBookSellUpdate {
        price_levels: Vec<PriceLevel>,
        best_ask: Option<String>,
        timestamp: String,
    },
    /// Order book full snapshot
    OrderBookSnapshot {
        bids: Vec<PriceLevel>,
        asks: Vec<PriceLevel>,
        best_bid: Option<String>,
        best_ask: Option<String>,
        mid_price: Option<String>,
        spread: Option<String>,
        timestamp: String,
    },
    /// Trade execution notification
    TradeExecuted {
        trade_id: String,
        buy_order_id: String,
        sell_order_id: String,
        buyer_id: String,
        seller_id: String,
        quantity: String,
        price: String,
        total_value: String,
        executed_at: String,
    },
    /// Market depth update
    MarketDepthUpdate {
        total_buy_volume: String,
        total_sell_volume: String,
        buy_orders_count: usize,
        sell_orders_count: usize,
        spread_percentage: Option<f64>,
    },

    /// Meter reading received event
    MeterReadingReceived {
        user_id: Uuid,
        wallet_address: String,
        meter_serial: String,
        kwh_amount: f64,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Tokens minted event
    TokensMinted {
        user_id: Uuid,
        wallet_address: String,
        meter_serial: String,
        kwh_amount: f64,
        tokens_minted: u64,
        transaction_signature: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Meter reading validation failed event
    MeterReadingValidationFailed {
        user_id: Uuid,
        wallet_address: String,
        meter_serial: String,
        kwh_amount: f64,
        error_reason: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },

    /// Batch minting completed event
    BatchMintingCompleted {
        batch_id: String,
        total_readings: u32,
        successful_mints: u32,
        failed_mints: u32,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
}

/// Price level for order book updates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceLevel {
    pub price: String,
    pub volume: String,
}

/// WebSocket client connection
#[allow(dead_code)]
struct Client {
    id: Uuid,
    sender: SplitSink<WebSocket, Message>,
}

/// WebSocket broadcast service
#[derive(Clone)]
pub struct WebSocketService {
    clients: Arc<RwLock<HashMap<Uuid, mpsc::UnboundedSender<MarketEvent>>>>,
}

impl WebSocketService {
    /// Create a new WebSocket service
    pub fn new() -> Self {
        info!("🔌 Initializing WebSocket service for real-time market updates");
        Self {
            clients: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new WebSocket client
    pub async fn register_client(&self, socket: WebSocket) -> Uuid {
        let client_id = Uuid::new_v4();
        let (sender, mut receiver) = socket.split();
        let (tx, mut rx) = mpsc::unbounded_channel::<MarketEvent>();

        // Store the client sender
        self.clients.write().await.insert(client_id, tx);

        info!("✅ WebSocket client connected: {}", client_id);

        // Spawn task to forward messages to this client
        let clients = self.clients.clone();
        tokio::spawn(async move {
            let mut sender = sender;

            // Send welcome message
            let welcome = serde_json::json!({
                "type": "connected",
                "client_id": client_id.to_string(),
                "message": "Connected to GridTokenX market feed"
            });

            if let Ok(json) = serde_json::to_string(&welcome) {
                let _ = sender.send(Message::Text(json.into())).await;
            }

            // Forward market events to this client
            while let Some(event) = rx.recv().await {
                match serde_json::to_string(&event) {
                    Ok(json) => {
                        if let Err(e) = sender.send(Message::Text(json.into())).await {
                            warn!("Failed to send message to client {}: {}", client_id, e);
                            break;
                        }
                    }
                    Err(e) => {
                        error!("Failed to serialize event: {}", e);
                    }
                }
            }

            // Client disconnected, clean up
            clients.write().await.remove(&client_id);
            info!("❌ WebSocket client disconnected: {}", client_id);
        });

        // Spawn task to handle incoming messages (ping/pong, subscriptions)
        tokio::spawn(async move {
            while let Some(Ok(msg)) = receiver.next().await {
                match msg {
                    Message::Text(text) => {
                        // Handle subscription messages if needed
                        info!("Received message from client: {}", text);
                    }
                    Message::Close(_) => {
                        info!("Client requested close");
                        break;
                    }
                    Message::Ping(_data) => {
                        // Handled automatically by axum
                    }
                    Message::Pong(_) => {}
                    _ => {}
                }
            }
        });

        client_id
    }

    /// Broadcast a market event to all connected clients
    pub async fn broadcast(&self, event: MarketEvent) {
        let clients = self.clients.read().await;
        let client_count = clients.len();

        if client_count == 0 {
            return; // No clients connected, skip broadcasting
        }

        info!(
            "📢 Broadcasting event to {} clients: {:?}",
            client_count, event
        );

        // Send to all clients
        for (client_id, tx) in clients.iter() {
            if let Err(e) = tx.send(event.clone()) {
                warn!("Failed to send event to client {}: {}", client_id, e);
            }
        }
    }

    /// Broadcast offer created event
    pub async fn broadcast_offer_created(
        &self,
        offer_id: String,
        energy_amount: f64,
        price_per_kwh: f64,
        energy_source: String,
        location: String,
        created_by: String,
    ) {
        self.broadcast(MarketEvent::OfferCreated {
            offer_id,
            energy_amount,
            price_per_kwh,
            energy_source,
            location,
            created_by,
        })
        .await;
    }

    /// Broadcast offer updated event
    pub async fn broadcast_offer_updated(
        &self,
        offer_id: String,
        status: String,
        energy_amount: Option<f64>,
    ) {
        self.broadcast(MarketEvent::OfferUpdated {
            offer_id,
            status,
            energy_amount,
        })
        .await;
    }

    /// Broadcast order created event
    pub async fn broadcast_order_created(
        &self,
        order_id: String,
        energy_amount: f64,
        max_price_per_kwh: f64,
        energy_source: Option<String>,
        created_by: String,
    ) {
        self.broadcast(MarketEvent::OrderCreated {
            order_id,
            energy_amount,
            max_price_per_kwh,
            energy_source,
            created_by,
        })
        .await;
    }

    /// Broadcast order matched event
    pub async fn broadcast_order_matched(
        &self,
        order_id: String,
        offer_id: String,
        transaction_id: String,
        matched_amount: f64,
        price_per_kwh: f64,
    ) {
        self.broadcast(MarketEvent::OrderMatched {
            order_id,
            offer_id,
            transaction_id,
            matched_amount,
            price_per_kwh,
        })
        .await;
    }

    /// Broadcast transaction updated event
    pub async fn broadcast_transaction_updated(
        &self,
        transaction_id: String,
        status: String,
        buyer_id: String,
        seller_id: String,
    ) {
        self.broadcast(MarketEvent::TransactionUpdated {
            transaction_id,
            status,
            buyer_id,
            seller_id,
        })
        .await;
    }

    /// Broadcast market statistics
    pub async fn broadcast_market_stats(
        &self,
        total_active_offers: i64,
        total_pending_orders: i64,
        average_price: f64,
        total_volume_24h: f64,
    ) {
        self.broadcast(MarketEvent::MarketStats {
            total_active_offers,
            total_pending_orders,
            average_price,
            total_volume_24h,
        })
        .await;
    }

    /// Get number of connected clients
    pub async fn client_count(&self) -> usize {
        self.clients.read().await.len()
    }

    /// Broadcast order book snapshot
    pub async fn broadcast_order_book_snapshot(
        &self,
        bids: Vec<(String, String)>,
        asks: Vec<(String, String)>,
        best_bid: Option<String>,
        best_ask: Option<String>,
        mid_price: Option<String>,
        spread: Option<String>,
    ) {
        let bids_levels: Vec<PriceLevel> = bids
            .into_iter()
            .map(|(price, volume)| PriceLevel { price, volume })
            .collect();

        let asks_levels: Vec<PriceLevel> = asks
            .into_iter()
            .map(|(price, volume)| PriceLevel { price, volume })
            .collect();

        self.broadcast(MarketEvent::OrderBookSnapshot {
            bids: bids_levels,
            asks: asks_levels,
            best_bid,
            best_ask,
            mid_price,
            spread,
            timestamp: chrono::Utc::now().to_rfc3339(),
        })
        .await;
    }

    /// Broadcast order book buy side update
    pub async fn broadcast_order_book_buy_update(
        &self,
        price_levels: Vec<(String, String)>,
        best_bid: Option<String>,
    ) {
        let levels: Vec<PriceLevel> = price_levels
            .into_iter()
            .map(|(price, volume)| PriceLevel { price, volume })
            .collect();

        self.broadcast(MarketEvent::OrderBookBuyUpdate {
            price_levels: levels,
            best_bid,
            timestamp: chrono::Utc::now().to_rfc3339(),
        })
        .await;
    }

    /// Broadcast order book sell side update
    pub async fn broadcast_order_book_sell_update(
        &self,
        price_levels: Vec<(String, String)>,
        best_ask: Option<String>,
    ) {
        let levels: Vec<PriceLevel> = price_levels
            .into_iter()
            .map(|(price, volume)| PriceLevel { price, volume })
            .collect();

        self.broadcast(MarketEvent::OrderBookSellUpdate {
            price_levels: levels,
            best_ask,
            timestamp: chrono::Utc::now().to_rfc3339(),
        })
        .await;
    }

    /// Broadcast trade execution
    /// Broadcast trade executed event
    pub async fn broadcast_trade_executed(
        &self,
        trade_id: String,
        buy_order_id: String,
        sell_order_id: String,
        buyer_id: String,
        seller_id: String,
        quantity: String,
        price: String,
        total_value: String,
        executed_at: String,
    ) {
        self.broadcast(MarketEvent::TradeExecuted {
            trade_id,
            buy_order_id,
            sell_order_id,
            buyer_id,
            seller_id,
            quantity,
            price,
            total_value,
            executed_at,
        })
        .await;
    }

    /// Broadcast market depth update
    pub async fn broadcast_market_depth_update(
        &self,
        total_buy_volume: String,
        total_sell_volume: String,
        buy_orders_count: usize,
        sell_orders_count: usize,
        spread_percentage: Option<f64>,
    ) {
        self.broadcast(MarketEvent::MarketDepthUpdate {
            total_buy_volume,
            total_sell_volume,
            buy_orders_count,
            sell_orders_count,
            spread_percentage,
        })
        .await;
    }

    /// Broadcast meter reading received event
    pub async fn broadcast_meter_reading_received(
        &self,
        user_id: &uuid::Uuid,
        wallet_address: &str,
        meter_serial: &str,
        kwh_amount: f64,
    ) {
        self.broadcast(MarketEvent::MeterReadingReceived {
            user_id: *user_id,
            wallet_address: wallet_address.to_string(),
            meter_serial: meter_serial.to_string(),
            kwh_amount,
            timestamp: chrono::Utc::now(),
        })
        .await;
    }

    /// Broadcast tokens minted event
    pub async fn broadcast_tokens_minted(
        &self,
        user_id: &uuid::Uuid,
        wallet_address: &str,
        meter_serial: &str,
        kwh_amount: f64,
        tokens_minted: u64,
        transaction_signature: &str,
    ) {
        self.broadcast(MarketEvent::TokensMinted {
            user_id: *user_id,
            wallet_address: wallet_address.to_string(),
            meter_serial: meter_serial.to_string(),
            kwh_amount,
            tokens_minted,
            transaction_signature: transaction_signature.to_string(),
            timestamp: chrono::Utc::now(),
        })
        .await;
    }

    /// Broadcast meter reading validation failed event
    pub async fn broadcast_meter_reading_validation_failed(
        &self,
        user_id: &uuid::Uuid,
        wallet_address: &str,
        meter_serial: &str,
        kwh_amount: f64,
        error_reason: &str,
    ) {
        self.broadcast(MarketEvent::MeterReadingValidationFailed {
            user_id: *user_id,
            wallet_address: wallet_address.to_string(),
            meter_serial: meter_serial.to_string(),
            kwh_amount,
            error_reason: error_reason.to_string(),
            timestamp: chrono::Utc::now(),
        })
        .await;
    }

    /// Broadcast batch minting completed event
    pub async fn broadcast_batch_minting_completed(
        &self,
        batch_id: &str,
        total_readings: u32,
        successful_mints: u32,
        failed_mints: u32,
    ) {
        self.broadcast(MarketEvent::BatchMintingCompleted {
            batch_id: batch_id.to_string(),
            total_readings,
            successful_mints,
            failed_mints,
            timestamp: chrono::Utc::now(),
        })
        .await;
    }
}

impl Default for WebSocketService {
    fn default() -> Self {
        Self::new()
    }
}
