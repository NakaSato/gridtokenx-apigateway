//! Kafka consumer service for high-throughput meter reading ingestion.
//!
//! Consumes meter readings from Kafka topics and pushes them to the Redis queue
//! for processing by the ReadingProcessorService.

use std::sync::Arc;
use std::time::Duration;

use rdkafka::config::ClientConfig;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::message::Message;
use rdkafka::error::KafkaError;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

use crate::app_state::AppState;
use crate::handlers::auth::types::{CreateReadingParams, CreateReadingRequest};
use crate::services::reading_processor::ReadingTask;

/// Configuration for the Kafka consumer service
#[derive(Debug, Clone)]
pub struct KafkaConsumerConfig {
    /// Kafka bootstrap servers (comma-separated)
    pub bootstrap_servers: String,
    /// Topic to consume from
    pub topic: String,
    /// Consumer group ID
    pub group_id: String,
    /// Whether Kafka consumer is enabled
    pub enabled: bool,
}

impl Default for KafkaConsumerConfig {
    fn default() -> Self {
        Self {
            bootstrap_servers: std::env::var("KAFKA_BOOTSTRAP_SERVERS")
                .unwrap_or_else(|_| "kafka:9092".to_string()),
            topic: std::env::var("KAFKA_TOPIC")
                .unwrap_or_else(|_| "meter-readings".to_string()),
            group_id: std::env::var("KAFKA_CONSUMER_GROUP")
                .unwrap_or_else(|_| "gridtokenx-apigateway".to_string()),
            enabled: std::env::var("KAFKA_ENABLED")
                .map(|v| v.to_lowercase() == "true")
                .unwrap_or(false),
        }
    }
}

/// Kafka meter reading payload (matches Python producer format)
#[derive(Debug, Serialize, Deserialize)]
pub struct KafkaMeterReading {
    pub meter_serial: String,
    #[serde(alias = "meter_id")]
    pub meter_id: Option<String>,
    pub kwh: f64,
    pub timestamp: String,
    pub energy_generated: Option<f64>,
    pub energy_consumed: Option<f64>,
    pub power_generated: Option<f64>,
    pub power_consumed: Option<f64>,
    pub voltage: Option<f64>,
    pub current: Option<f64>,
    pub frequency: Option<f64>,
    pub power_factor: Option<f64>,
    pub thd_voltage: Option<f64>,
    pub thd_current: Option<f64>,
    pub zone_id: Option<i32>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub battery_level: Option<f64>,
}

/// Kafka consumer service that ingests meter readings
pub struct KafkaConsumerService {
    config: KafkaConsumerConfig,
}

impl KafkaConsumerService {
    /// Create a new Kafka consumer service
    pub fn new(config: KafkaConsumerConfig) -> Self {
        Self { config }
    }

    /// Create with default configuration from environment
    pub fn from_env() -> Self {
        Self::new(KafkaConsumerConfig::default())
    }

    /// Check if Kafka consumer is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Start the Kafka consumer loop
    pub async fn start(&self, app_state: Arc<AppState>) -> Result<(), KafkaError> {
        if !self.config.enabled {
            info!("⏸️ Kafka consumer is disabled (KAFKA_ENABLED=false)");
            return Ok(());
        }

        info!(
            "🚀 Starting Kafka consumer: servers={}, topic={}, group={}",
            self.config.bootstrap_servers, self.config.topic, self.config.group_id
        );

        // Create consumer
        let consumer: StreamConsumer = ClientConfig::new()
            .set("group.id", &self.config.group_id)
            .set("bootstrap.servers", &self.config.bootstrap_servers)
            .set("enable.auto.commit", "true")
            .set("auto.commit.interval.ms", "5000")
            .set("auto.offset.reset", "latest")
            .set("session.timeout.ms", "30000")
            .set("heartbeat.interval.ms", "10000")
            .create()?;

        // Subscribe to topic
        consumer.subscribe(&[&self.config.topic])?;
        info!("✅ Kafka consumer subscribed to topic: {}", self.config.topic);

        // Statistics
        let mut messages_received: u64 = 0;
        let mut messages_processed: u64 = 0;
        let mut messages_failed: u64 = 0;

        // Consume messages
        let mut stream = consumer.stream();
        
        loop {
            match stream.next().await {
                Some(Ok(message)) => {
                    messages_received += 1;
                    
                    if let Some(payload) = message.payload() {
                        match serde_json::from_slice::<KafkaMeterReading>(payload) {
                            Ok(reading) => {
                                let meter_serial = reading.meter_serial.clone();
                                debug!(
                                    "📥 Kafka received: {} ({:.3} kWh)",
                                    meter_serial, reading.kwh
                                );

                                // Convert to ReadingTask and push to Redis queue
                                match self.convert_to_reading_task(&reading) {
                                    Ok(task) => {
                                        if let Err(e) = app_state.cache_service.push_reading(&task).await {
                                            error!("❌ Failed to queue Kafka reading: {}", e);
                                            messages_failed += 1;
                                        } else {
                                            messages_processed += 1;
                                            metrics::counter!("kafka_messages_processed").increment(1);
                                        }
                                    }
                                    Err(e) => {
                                        warn!("⚠️ Failed to convert Kafka message: {}", e);
                                        messages_failed += 1;
                                    }
                                }
                            }
                            Err(e) => {
                                warn!(
                                    "⚠️ Failed to parse Kafka message: {} - payload: {:?}",
                                    e,
                                    String::from_utf8_lossy(payload)
                                );
                                messages_failed += 1;
                            }
                        }
                    }

                    // Log stats periodically
                    if messages_received % 100 == 0 {
                        info!(
                            "📊 Kafka stats: received={}, processed={}, failed={}",
                            messages_received, messages_processed, messages_failed
                        );
                    }
                }
                Some(Err(e)) => {
                    error!("❌ Kafka consumer error: {}", e);
                    metrics::counter!("kafka_consumer_errors").increment(1);
                    // Brief pause before continuing
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                None => {
                    warn!("⚠️ Kafka stream ended unexpectedly, reconnecting...");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    break;
                }
            }
        }

        Ok(())
    }

    /// Convert Kafka reading to ReadingTask for queue processing
    fn convert_to_reading_task(&self, reading: &KafkaMeterReading) -> Result<ReadingTask, String> {
        let meter_serial = reading.meter_serial.clone();
        
        // Build CreateReadingParams
        let params = CreateReadingParams::default();

        // Build CreateReadingRequest from Kafka payload
        let mut request = CreateReadingRequest::default();
        request.kwh = reading.kwh;
        request.timestamp = chrono::DateTime::parse_from_rfc3339(&reading.timestamp)
            .ok()
            .map(|dt| dt.with_timezone(&chrono::Utc));
        request.voltage = reading.voltage;
        request.current = reading.current;
        request.power_factor = reading.power_factor;
        request.frequency = reading.frequency;
        request.energy_generated = reading.energy_generated;
        request.energy_consumed = reading.energy_consumed;
        request.power_generated = reading.power_generated;
        request.power_consumed = reading.power_consumed;
        request.thd_voltage = reading.thd_voltage;
        request.thd_current = reading.thd_current;
        request.battery_level = reading.battery_level;
        request.zone_id = reading.zone_id;
        request.latitude = reading.latitude;
        request.longitude = reading.longitude;
        request.meter_serial = Some(meter_serial.clone());

        Ok(ReadingTask {
            serial: meter_serial,
            params,
            request,
            retry_count: 0,
        })
    }
}

impl Clone for KafkaConsumerService {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
        }
    }
}
