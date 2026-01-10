pub mod types;
 
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::Utc;
use crate::services::websocket::WebSocketService;
use crate::services::event_processor::EventProcessorService;
use crate::services::health_check::HealthChecker;
use crate::services::transaction::metrics::MetricsExporter;
use std::collections::HashMap;
pub use types::{DashboardMetrics, GridStatus, ZoneGridStatus};
use crate::services::websocket::types::ZoneStatus as WsZoneStatus;

#[derive(Clone)]
pub struct DashboardService {
    db: sqlx::PgPool,
    health_checker: HealthChecker,
    event_processor: EventProcessorService,
    websocket_service: WebSocketService,
    metrics: Arc<RwLock<GridStatus>>,
}

impl DashboardService {
    pub fn new(
        db: sqlx::PgPool,
        health_checker: HealthChecker,
        event_processor: EventProcessorService,
        websocket_service: WebSocketService,
    ) -> Self {
        Self {
            db,
            health_checker,
            event_processor,
            websocket_service,
                metrics: Arc::new(RwLock::new(GridStatus {
                total_generation: 0.0,
                total_consumption: 0.0,
                net_balance: 0.0,
                active_meters: 0,
                co2_saved_kg: 0.0,
                zones: HashMap::new(),
                zones_data: None,
                timestamp: Utc::now(),
            })),
        }
    }

    /// Handle a new meter reading to update aggregate grid status and broadcast
    pub async fn handle_meter_reading(&self, kwh: f64, _meter_serial: &str, zone_id: Option<i32>) -> anyhow::Result<()> {
        let mut metrics = self.metrics.write().await;
        
        // Update aggregate totals
        if kwh > 0.0 {
            metrics.total_generation += kwh;
        } else {
            metrics.total_consumption += kwh.abs();
        }

        // Update zone-specific totals if zone_id is provided
        if let Some(zid) = zone_id {
            let zone_status = metrics.zones.entry(zid).or_insert(ZoneGridStatus {
                zone_id: zid,
                generation: 0.0,
                consumption: 0.0,
                net_balance: 0.0,
                active_meters: 0,
            });

            if kwh > 0.0 {
                zone_status.generation += kwh;
            } else {
                zone_status.consumption += kwh.abs();
            }
            zone_status.net_balance = zone_status.generation - zone_status.consumption;
            
            // Simple increment for now, similar to global logic
            if zone_status.active_meters < 10 {
                zone_status.active_meters += 1;
            }
        }

        // Increment active meters if it was 0 or just maintain (simple logic for now)
        if metrics.active_meters < 30 { 
             metrics.active_meters += 1;
        }

        metrics.net_balance = metrics.total_generation - metrics.total_consumption;
        metrics.co2_saved_kg = metrics.total_generation * 0.431;
        metrics.timestamp = Utc::now();

        // Broadcast to all connected clients
        let ws = self.websocket_service.clone();
        let gen = metrics.total_generation;
        let cons = metrics.total_consumption;
        let bal = metrics.net_balance;
        let active = metrics.active_meters;
        let co2 = metrics.co2_saved_kg;
        
        // Map Dashboard zone status to WebSocket zone status
        let ws_zones: HashMap<i32, WsZoneStatus> = metrics.zones.iter().map(|(id, z)| {
            (*id, WsZoneStatus {
                zone_id: z.zone_id,
                generation: z.generation,
                consumption: z.consumption,
                net_balance: z.net_balance,
                active_meters: z.active_meters,
            })
        }).collect();

        tokio::spawn(async move {
            ws.broadcast_grid_status_updated(gen, cons, bal, active, co2, ws_zones)
                .await;
        });

        Ok(())
    }

    pub async fn get_grid_status(&self) -> GridStatus {
        let metrics: tokio::sync::RwLockReadGuard<'_, GridStatus> = self.metrics.read().await;
        metrics.clone()
    }

    /// Retrieve historical grid status snapshots
    pub async fn get_grid_history(&self, limit: i64) -> anyhow::Result<Vec<GridStatus>> {
        let history = sqlx::query_as::<_, GridStatus>(
            "SELECT total_generation, total_consumption, net_balance, active_meters, co2_saved_kg, timestamp, zones_data
             FROM grid_status_history 
             ORDER BY timestamp DESC 
             LIMIT $1"
        )
        .bind(limit)
        .fetch_all(&self.db)
        .await?;

        // Populate zones from zones_data JSONB
        let mapped_history = history.into_iter().map(|mut gs| {
            if let Some(zd) = gs.zones_data.take() {
                if let Ok(zones) = serde_json::from_value::<HashMap<i32, ZoneGridStatus>>(zd) {
                    gs.zones = zones;
                }
            }
            gs
        }).collect();

        Ok(mapped_history)
    }

    /// Start a background task to record grid status snapshots periodically
    pub async fn start_history_recorder(&self) {
        let self_clone = self.clone();
        let interval_secs = std::env::var("GRID_HISTORY_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(60); // Default to 1 minute

        tokio::spawn(async move {
            tracing::info!("🚀 Starting Grid History Recorder (interval: {}s)", interval_secs);
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
            
            loop {
                interval.tick().await;
                
                let current = self_clone.get_grid_status().await;
                let snapshot_time = Utc::now();
                let zones_json = serde_json::to_value(&current.zones).unwrap_or(serde_json::Value::Null);
                
                // Only record if there's some activity or regularly
                let result = sqlx::query(
                    "INSERT INTO grid_status_history (total_generation, total_consumption, net_balance, active_meters, co2_saved_kg, timestamp, zones_data)
                     VALUES ($1, $2, $3, $4, $5, $6, $7)"
                )
                .bind(current.total_generation)
                .bind(current.total_consumption)
                .bind(current.net_balance)
                .bind(current.active_meters)
                .bind(current.co2_saved_kg)
                .bind(snapshot_time)
                .bind(zones_json)
                .execute(&self_clone.db)
                .await;

                if let Err(e) = result {
                    tracing::error!("❌ Failed to record grid history snapshot: {}", e);
                }
            }
        });
    }

    pub async fn get_metrics(&self) -> anyhow::Result<DashboardMetrics> {
        // Fetch metrics in parallel where possible
        let (health_status, event_stats) = tokio::join!(
            self.health_checker.perform_health_check(),
            self.event_processor.get_stats()
        );

        let pending_transactions = MetricsExporter::get_transaction_stats();

        Ok(DashboardMetrics {
            system_health: health_status,
            event_processor: event_stats?,
            pending_transactions,
            grid_status: self.get_grid_status().await,
        })
    }
}
