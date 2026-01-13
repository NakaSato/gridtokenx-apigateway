use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tracing::{info, error, debug, warn};
use serde::{Deserialize, Serialize};

use crate::app_state::AppState;
use crate::handlers::auth::types::{CreateReadingRequest, CreateReadingParams};
use crate::handlers::auth::meters::process_reading_task;

/// Task representing a meter reading to be processed asynchronously
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReadingTask {
    pub serial: String,
    pub params: CreateReadingParams,
    pub request: CreateReadingRequest,
    #[serde(default)]
    pub retry_count: u32,
}

/// Service that processes meter readings from a Redis queue
#[derive(Clone, Default)]
pub struct ReadingProcessorService;

impl ReadingProcessorService {
    /// Create a new reading processor service
    pub fn new() -> Self {
        Self
    }

    /// Start the background worker loop
    pub async fn start(&self, app_state: Arc<AppState>, worker_id: u32) {
        info!("🚀 Starting Reading Processor background worker #{}", worker_id);
        
        loop {
            // Periodically check queue depth (only worker #0 handles this to avoid duplicate metrics)
            if worker_id == 0 {
                if let Ok(depth) = app_state.cache_service.get_queue_depth("queue:meter_readings").await {
                    metrics::gauge!("meter_processing_queue_depth").set(depth as f64);
                    if depth > 1000 {
                        warn!("⚠️ High queue depth detected: {} readings pending", depth);
                    }
                }
            }

            match app_state.cache_service.pop_reading::<ReadingTask>().await {
                Ok(Some(mut task)) => {
                    let serial = task.serial.clone();
                    let start_time = std::time::Instant::now();
                    debug!("[Worker #{}] 📥 Popped reading for {} from queue (retry: {})", worker_id, serial, task.retry_count);
                    
                    let state = app_state.clone();
                    
                    // Process reading using the refactored logic in meters.rs
                    match process_reading_task(&state, task.clone()).await {
                        Ok(_) => {
                            let duration = start_time.elapsed();
                            crate::middleware::metrics::track_meter_reading(true);
                            metrics::histogram!("meter_processing_duration_seconds").record(duration.as_secs_f64());
                            debug!("[Worker #{}] ✅ Processed {} in {:?}", worker_id, serial, duration);
                        }
                        Err(e) => {
                            let _duration = start_time.elapsed();
                            error!("[Worker #{}] ❌ Failed to process {} (attempt {}): {}", worker_id, serial, task.retry_count + 1, e);
                            
                            task.retry_count += 1;
                            if task.retry_count < 3 {
                                // Exponential backoff for retries: 2, 4, 8 seconds
                                let backoff = Duration::from_secs(2u64.pow(task.retry_count));
                                warn!("[Worker #{}] 🔄 Re-queueing {} for retry in {:?}", worker_id, serial, backoff);
                                
                                // In a simple implementation, we just push it back. 
                                // In a more robust one, we'd use a delayed queue or ZSET.
                                // For now, we push to the end (RPUSH) so it doesn't block the frontend.
                                if let Err(push_err) = state.cache_service.push_reading(&task).await {
                                    error!("❌ Critical: Failed to re-queue task: {}", push_err);
                                }
                                crate::middleware::metrics::track_meter_reading_retry();
                            } else {
                                error!("[Worker #{}] 💀 Task for {} exceeded max retries. Moving to DLQ.", worker_id, serial);
                                if let Err(dlq_err) = state.cache_service.push_to_dlq(&task).await {
                                    error!("❌ Critical: Failed to push to DLQ: {}", dlq_err);
                                }
                                crate::middleware::metrics::track_meter_reading(false);
                            }
                        }
                    }
                }
                Ok(None) => {
                    // BRPOP already waits for 1s
                    tokio::task::yield_now().await;
                }
                Err(e) => {
                    error!("[Worker #{}] ❌ Error popping reading from queue: {}", worker_id, e);
                    sleep(Duration::from_secs(5)).await;
                }
            }
        }
    }
}
