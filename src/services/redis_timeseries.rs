// Redis Time Series Service for GridTokenX
// Implements time-series data storage and analytics for energy trading

use redis::{AsyncCommands, Client, RedisError, RedisResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info, warn};

/// Time series data point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSeriesPoint {
    pub timestamp: i64,
    pub value: f64,
    pub labels: Option<HashMap<String, String>>,
}

impl TimeSeriesPoint {
    /// Create a new time series point
    pub fn new(timestamp: i64, value: f64) -> Self {
        Self {
            timestamp,
            value,
            labels: None,
        }
    }
    
    /// Create a new time series point with labels
    pub fn with_labels(timestamp: i64, value: f64, labels: HashMap<String, String>) -> Self {
        Self {
            timestamp,
            value,
            labels: Some(labels),
        }
    }
    
    /// Create a point from current time
    pub fn now(value: f64) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        Self::new(timestamp, value)
    }
    
    /// Create a point from current time with labels
    pub fn now_with_labels(value: f64, labels: HashMap<String, String>) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        Self::with_labels(timestamp, value, labels)
    }
}

/// Time series aggregation functions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Aggregation {
    /// Average of values
    Avg,
    /// Sum of values
    Sum,
    /// Minimum value
    Min,
    /// Maximum value
    Max,
    /// Count of values
    Count,
    /// Standard deviation
    StdDev,
    /// First value
    First,
    /// Last value
    Last,
}

/// Time series query range
#[derive(Debug, Clone)]
pub struct TimeRange {
    pub start: i64,
    pub end: i64,
}

impl TimeRange {
    /// Create a new time range
    pub fn new(start: i64, end: i64) -> Self {
        Self { start, end }
    }
    
    /// Create a range for the last N milliseconds
    pub fn last_milliseconds(ms: i64) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        Self {
            start: now - ms,
            end: now,
        }
    }
    
    /// Create a range for the last N seconds
    pub fn last_seconds(seconds: i64) -> Self {
        Self::last_milliseconds(seconds * 1000)
    }
    
    /// Create a range for the last N minutes
    pub fn last_minutes(minutes: i64) -> Self {
        Self::last_seconds(minutes * 60)
    }
    
    /// Create a range for the last N hours
    pub fn last_hours(hours: i64) -> Self {
        Self::last_minutes(hours * 60)
    }
    
    /// Create a range for the last N days
    pub fn last_days(days: i64) -> Self {
        Self::last_hours(days * 24)
    }
}

/// Time series service for GridTokenX
pub struct RedisTimeSeriesService {
    client: Client,
}

impl RedisTimeSeriesService {
    /// Create a new time series service
    pub fn new(redis_url: &str) -> RedisResult<Self> {
        let client = Client::open(redis_url)?;
        Ok(Self { client })
    }
    
    /// Create a time series key
    pub async fn create_time_series(
        &self,
        key: &str,
        retention_ms: Option<u64>,
        labels: Option<HashMap<String, String>>,
    ) -> RedisResult<bool> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        
        // Using Redis sorted sets as time series (basic implementation)
        // In production, you'd use RedisTimeSeries module
        
        let mut cmd = redis::Cmd::new();
        cmd.arg("TS.CREATE")
            .arg(key);
        
        if let Some(retention) = retention_ms {
            cmd.arg("RETENTION").arg(retention);
        }
        
        if let Some(labels_map) = labels {
            cmd.arg("LABELS");
            for (label, value) in labels_map {
                cmd.arg(label).arg(value);
            }
        }
        
        // Try to create with RedisTimeSeries, fallback to sorted set
        match conn.query::<()>(&cmd) {
            Ok(_) => {
                info!("Created time series: {}", key);
                Ok(true)
            }
            Err(_) => {
                // Fallback: initialize sorted set
                let _: () = conn.zadd(key, 0, 0).await?;
                info!("Created fallback time series (sorted set): {}", key);
                Ok(true)
            }
        }
    }
    
    /// Add a data point to time series
    pub async fn add_point(&self, key: &str, point: &TimeSeriesPoint) -> RedisResult<bool> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        
        // Try RedisTimeSeries first
        let mut cmd = redis::Cmd::new();
        cmd.arg("TS.ADD")
            .arg(key)
            .arg(point.timestamp)
            .arg(point.value);
        
        if let Some(ref labels) = point.labels {
            cmd.arg("LABELS");
            for (label, value) in labels {
                cmd.arg(label).arg(value);
            }
        }
        
        match conn.query::<()>(&cmd) {
            Ok(_) => {
                debug!("Added point to time series {}: {}", key, point.value);
                Ok(true)
            }
            Err(_) => {
                // Fallback to sorted set
                let _: () = conn.zadd(key, &point.value, &point.timestamp).await?;
                debug!("Added point to fallback time series {}: {}", key, point.value);
                Ok(true)
            }
        }
    }
    
    /// Add multiple data points
    pub async fn add_points(&self, key: &str, points: &[TimeSeriesPoint]) -> RedisResult<u32> {
        let mut success_count = 0u32;
        
        for point in points {
            if self.add_point(key, point).await.unwrap_or(false) {
                success_count += 1;
            }
        }
        
        info!("Added {} points to time series {}", success_count, key);
        Ok(success_count)
    }
    
    /// Query time series data
    pub async fn query_range(
        &self,
        key: &str,
        range: &TimeRange,
        aggregation: Option<(&Aggregation, u64)>, // (aggregation, time_bucket_ms)
    ) -> RedisResult<Vec<TimeSeriesPoint>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        
        // Try RedisTimeSeries query
        let mut cmd = redis::Cmd::new();
        cmd.arg("TS.RANGE")
            .arg(key)
            .arg(range.start)
            .arg(range.end);
        
        if let Some((agg, bucket)) = aggregation {
            cmd.arg("AGGREGATION");
            let agg_str = match agg {
                Aggregation::Avg => "AVG",
                Aggregation::Sum => "SUM",
                Aggregation::Min => "MIN",
                Aggregation::Max => "MAX",
                Aggregation::Count => "COUNT",
                Aggregation::StdDev => "STDDEV",
                Aggregation::First => "FIRST",
                Aggregation::Last => "LAST",
            };
            cmd.arg(agg_str).arg(bucket);
        }
        
        match conn.query::<Vec<Vec<serde_json::Value>>>(&cmd) {
            Ok(results) => {
                let points: Vec<TimeSeriesPoint> = results
                    .into_iter()
                    .filter_map(|item| {
                        if item.len() >= 2 {
                            let timestamp = item[0].as_i64().unwrap_or(0);
                            let value = item[1].as_f64().unwrap_or(0.0);
                            Some(TimeSeriesPoint::new(timestamp, value))
                        } else {
                            None
                        }
                    })
                    .collect();
                
                debug!("Queried {} points from time series {}", points.len(), key);
                Ok(points)
            }
            Err(_) => {
                // Fallback to sorted set range query
                let results: Vec<(String, f64)> = conn
                    .zrangebyscore_withscores(key, range.start, range.end)
                    .await?;
                
                let points: Vec<TimeSeriesPoint> = results
                    .into_iter()
                    .map(|(timestamp, value)| {
                        let ts = timestamp.parse::<i64>().unwrap_or(0);
                        TimeSeriesPoint::new(ts, value)
                    })
                    .collect();
                
                debug!("Queried {} points from fallback time series {}", points.len(), key);
                Ok(points)
            }
        }
    }
    
    /// Get the latest value from time series
    pub async fn get_latest(&self, key: &str) -> RedisResult<Option<TimeSeriesPoint>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        
        // Try RedisTimeSeries GET
        let cmd = redis::Cmd::new()
            .arg("TS.GET")
            .arg(key);
        
        match conn.query::<Option<Vec<serde_json::Value>>>(&cmd) {
            Ok(Some(result)) => {
                if result.len() >= 2 {
                    let timestamp = result[0].as_i64().unwrap_or(0);
                    let value = result[1].as_f64().unwrap_or(0.0);
                    Ok(Some(TimeSeriesPoint::new(timestamp, value)))
                } else {
                    Ok(None)
                }
            }
            Ok(None) => Ok(None),
            Err(_) => {
                // Fallback to sorted set
                let result: Option<(String, f64)> = conn.zrevrangebyscore_withscores(key, "+inf", "-inf", 1, 0).await?;
                
                match result {
                    Some((timestamp, value)) => {
                        let ts = timestamp.parse::<i64>().unwrap_or(0);
                        Ok(Some(TimeSeriesPoint::new(ts, value)))
                    }
                    None => Ok(None),
                }
            }
        }
    }
    
    /// Get statistics for time series
    pub async fn get_stats(&self, key: &str, range: &TimeRange) -> RedisResult<TimeSeriesStats> {
        let points = self.query_range(key, range, None).await?;
        
        if points.is_empty() {
            return Ok(TimeSeriesStats::default());
        }
        
        let values: Vec<f64> = points.iter().map(|p| p.value).collect();
        let count = values.len() as u64;
        let sum: f64 = values.iter().sum();
        let avg = sum / count as f64;
        let min = values.iter().fold(f64::INFINITY, |a, &b| a.min(b));
        let max = values.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
        
        // Calculate standard deviation
        let variance = values.iter()
            .map(|&x| (x - avg).powi(2))
            .sum::<f64>() / count as f64;
        let std_dev = variance.sqrt();
        
        Ok(TimeSeriesStats {
            count,
            sum,
            avg,
            min,
            max,
            std_dev,
            first_timestamp: points.first().map(|p| p.timestamp),
            last_timestamp: points.last().map(|p| p.timestamp),
        })
    }
    
    /// Delete time series
    pub async fn delete_time_series(&self, key: &str) -> RedisResult<bool> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        
        let result: i32 = conn.del(key).await?;
        let deleted = result > 0;
        
        if deleted {
            info!("Deleted time series: {}", key);
        } else {
            warn!("Time series not found for deletion: {}", key);
        }
        
        Ok(deleted)
    }
    
    /// List all time series keys
    pub async fn list_time_series(&self, pattern: &str) -> RedisResult<Vec<String>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        
        let keys: Vec<String> = conn.keys(pattern).await?;
        debug!("Found {} time series matching pattern: {}", keys.len(), pattern);
        Ok(keys)
    }
    
    /// Compact time series (reduce data points)
    pub async fn compact(
        &self,
        source_key: &str,
        target_key: &str,
        aggregation: &Aggregation,
        bucket_ms: u64,
        range: Option<TimeRange>,
    ) -> RedisResult<u32> {
        let query_range = range.unwrap_or_else(|| TimeRange::last_days(30));
        let points = self.query_range(source_key, &query_range, Some((aggregation, bucket_ms))).await?;
        
        if !self.create_time_series(target_key, None, None).await.unwrap_or(false) {
            warn!("Failed to create compacted time series: {}", target_key);
        }
        
        let added = self.add_points(target_key, &points).await?;
        info!("Compacted {} points from {} to {}", added, source_key, target_key);
        Ok(added)
    }
}

/// Time series statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TimeSeriesStats {
    pub count: u64,
    pub sum: f64,
    pub avg: f64,
    pub min: f64,
    pub max: f64,
    pub std_dev: f64,
    pub first_timestamp: Option<i64>,
    pub last_timestamp: Option<i64>,
}

/// Pre-configured time series service for GridTokenX
pub struct GridTokenXTimeSeries {
    service: RedisTimeSeriesService,
}

impl GridTokenXTimeSeries {
    /// Create GridTokenX time series service
    pub fn new(redis_url: &str) -> RedisResult<Self> {
        let service = RedisTimeSeriesService::new(redis_url)?;
        Ok(Self { service })
    }
    
    /// Initialize all GridTokenX time series
    pub async fn initialize(&self) -> RedisResult<()> {
        // Market price time series
        let mut market_labels = HashMap::new();
        market_labels.insert("type".to_string(), "market_price".to_string());
        
        self.service.create_time_series(
            "market:price:gridtoken",
            Some(90 * 24 * 60 * 60 * 1000), // 90 days retention
            Some(market_labels),
        ).await?;
        
        // Trading volume time series
        let mut volume_labels = HashMap::new();
        volume_labels.insert("type".to_string(), "trading_volume".to_string());
        
        self.service.create_time_series(
            "trading:volume:daily",
            Some(365 * 24 * 60 * 60 * 1000), // 1 year retention
            Some(volume_labels),
        ).await?;
        
        // Energy generation time series
        let mut generation_labels = HashMap::new();
        generation_labels.insert("type".to_string(), "energy_generation".to_string());
        
        self.service.create_time_series(
            "energy:generation:hourly",
            Some(730 * 24 * 60 * 60 * 1000), // 2 years retention
            Some(generation_labels),
        ).await?;
        
        // Energy consumption time series
        let mut consumption_labels = HashMap::new();
        consumption_labels.insert("type".to_string(), "energy_consumption".to_string());
        
        self.service.create_time_series(
            "energy:consumption:hourly",
            Some(730 * 24 * 60 * 60 * 1000), // 2 years retention
            Some(consumption_labels),
        ).await?;
        
        // Settlement time series
        let mut settlement_labels = HashMap::new();
        settlement_labels.insert("type".to_string(), "settlement".to_string());
        
        self.service.create_time_series(
            "settlement:daily",
            Some(365 * 24 * 60 * 60 * 1000), // 1 year retention
            Some(settlement_labels),
        ).await?;
        
        info!("Initialized all GridTokenX time series");
        Ok(())
    }
    
    /// Record market price
    pub async fn record_market_price(&self, symbol: &str, price: f64) -> RedisResult<bool> {
        let key = format!("market:price:{}", symbol);
        let mut labels = HashMap::new();
        labels.insert("symbol".to_string(), symbol.to_string());
        
        let point = TimeSeriesPoint::now_with_labels(price, labels);
        self.service.add_point(&key, &point).await
    }
    
    /// Record trading volume
    pub async fn record_trading_volume(&self, volume: f64) -> RedisResult<bool> {
        let point = TimeSeriesPoint::now(volume);
        self.service.add_point("trading:volume:daily", &point).await
    }
    
    /// Record energy generation
    pub async fn record_energy_generation(&self, meter_id: &str, generation_kwh: f64) -> RedisResult<bool> {
        let key = format!("energy:generation:{}", meter_id);
        let mut labels = HashMap::new();
        labels.insert("meter_id".to_string(), meter_id.to_string());
        
        let point = TimeSeriesPoint::now_with_labels(generation_kwh, labels);
        self.service.add_point(&key, &point).await
    }
    
    /// Record energy consumption
    pub async fn record_energy_consumption(&self, meter_id: &str, consumption_kwh: f64) -> RedisResult<bool> {
        let key = format!("energy:consumption:{}", meter_id);
        let mut labels = HashMap::new();
        labels.insert("meter_id".to_string(), meter_id.to_string());
        
        let point = TimeSeriesPoint::now_with_labels(consumption_kwh, labels);
        self.service.add_point(&key, &point).await
    }
    
    /// Record settlement amount
    pub async fn record_settlement(&self, settlement_type: &str, amount: f64) -> RedisResult<bool> {
        let key = format!("settlement:{}", settlement_type);
        let mut labels = HashMap::new();
        labels.insert("type".to_string(), settlement_type.to_string());
        
        let point = TimeSeriesPoint::now_with_labels(amount, labels);
        self.service.add_point(&key, &point).await
    }
    
    /// Get market price history
    pub async fn get_market_price_history(
        &self,
        symbol: &str,
        range: &TimeRange,
    ) -> RedisResult<Vec<TimeSeriesPoint>> {
        let key = format!("market:price:{}", symbol);
        self.service.query_range(&key, range, None).await
    }
    
    /// Get trading volume statistics
    pub async fn get_trading_volume_stats(&self, range: &TimeRange) -> RedisResult<TimeSeriesStats> {
        self.service.get_stats("trading:volume:daily", range).await
    }
    
    /// Get energy generation statistics
    pub async fn get_energy_generation_stats(
        &self,
        meter_id: &str,
        range: &TimeRange,
    ) -> RedisResult<TimeSeriesStats> {
        let key = format!("energy:generation:{}", meter_id);
        self.service.get_stats(&key, range).await
    }
    
    /// Get energy consumption statistics
    pub async fn get_energy_consumption_stats(
        &self,
        meter_id: &str,
        range: &TimeRange,
    ) -> RedisResult<TimeSeriesStats> {
        let key = format!("energy:consumption:{}", meter_id);
        self.service.get_stats(&key, range).await
    }
    
    /// Get settlement statistics
    pub async fn get_settlement_stats(
        &self,
        settlement_type: &str,
        range: &TimeRange,
    ) -> RedisResult<TimeSeriesStats> {
        let key = format!("settlement:{}", settlement_type);
        self.service.get_stats(&key, range).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_time_series_point_creation() {
        let point = TimeSeriesPoint::new(1609459200000, 100.5);
        assert_eq!(point.timestamp, 1609459200000);
        assert_eq!(point.value, 100.5);
        assert!(point.labels.is_none());
        
        let mut labels = HashMap::new();
        labels.insert("test".to_string(), "value".to_string());
        let labeled_point = TimeSeriesPoint::with_labels(1609459200000, 100.5, labels);
        assert!(labeled_point.labels.is_some());
    }
    
    #[test]
    fn test_time_range_creation() {
        let range = TimeRange::new(1609459200000, 1609545600000);
        assert_eq!(range.start, 1609459200000);
        assert_eq!(range.end, 1609545600000);
        
        let last_24h = TimeRange::last_hours(24);
        assert!(last_24h.end > last_24h.start);
        assert!(last_24h.end - last_24h.start <= 24 * 60 * 60 * 1000 + 1000); // Allow 1s tolerance
    }
    
    #[test]
    fn test_time_series_stats_default() {
        let stats = TimeSeriesStats::default();
        assert_eq!(stats.count, 0);
        assert_eq!(stats.sum, 0.0);
        assert_eq!(stats.avg, 0.0);
    }
}
