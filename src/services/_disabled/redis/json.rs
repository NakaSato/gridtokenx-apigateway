// Redis JSON Service for GridTokenX
// Implements JSON data storage with advanced querying and manipulation

use redis::{AsyncCommands, Client, RedisResult};

use serde_json::{json, Value};

use tracing::{debug, info, warn};

/// JSON service for GridTokenX
pub struct RedisJSONService {
    client: Client,
}

impl RedisJSONService {
    /// Create a new JSON service
    pub fn new(redis_url: &str) -> RedisResult<Self> {
        let client = Client::open(redis_url)?;
        Ok(Self { client })
    }

    /// Set JSON value at path
    pub async fn json_set(&self, key: &str, path: &str, value: &Value) -> RedisResult<bool> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;

        // Try RedisJSON module first
        let mut cmd = redis::Cmd::new();
        cmd.arg("JSON.SET")
            .arg(key)
            .arg(path)
            .arg(value.to_string());

        match cmd.query_async::<()>(&mut conn).await {
            Ok(_) => {
                debug!("Set JSON at {}: {}", key, path);
                Ok(true)
            }
            Err(_) => {
                // Fallback: store as string
                let json_key = format!("json:{}", key);
                let _: () = conn.set(&json_key, value.to_string()).await?;
                debug!("Set JSON fallback at {}: {}", json_key, path);
                Ok(true)
            }
        }
    }

    /// Get JSON value at path
    pub async fn json_get(&self, key: &str, path: &str) -> RedisResult<Option<Value>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;

        // Try RedisJSON module first
        let mut cmd = redis::Cmd::new();
        cmd.arg("JSON.GET").arg(key).arg(path);

        match cmd.query_async::<Option<String>>(&mut conn).await {
            Ok(Some(json_str)) => {
                let value: Value = serde_json::from_str(&json_str).unwrap_or_else(|_| json!(null));
                debug!("Got JSON at {}: {}", key, path);
                Ok(Some(value))
            }
            Ok(None) => Ok(None),
            Err(_) => {
                // Fallback: get as string
                let json_key = format!("json:{}", key);
                let json_str: Option<String> = conn.get(&json_key).await?;

                match json_str {
                    Some(str_val) => {
                        let value: Value =
                            serde_json::from_str(&str_val).unwrap_or_else(|_| json!(null));
                        debug!("Got JSON fallback at {}: {}", json_key, path);
                        Ok(Some(value))
                    }
                    None => Ok(None),
                }
            }
        }
    }

    /// Delete JSON value at path
    pub async fn json_del(&self, key: &str, path: &str) -> RedisResult<u32> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;

        // Try RedisJSON module first
        let mut cmd = redis::Cmd::new();
        cmd.arg("JSON.DEL").arg(key).arg(path);

        match cmd.query_async::<u32>(&mut conn).await {
            Ok(result) => {
                debug!("Deleted JSON at {}: {} ({} keys)", key, path, result);
                Ok(result)
            }
            Err(_) => {
                // Fallback: delete the entire key if path is root
                if path == "$" || path == "." {
                    let json_key = format!("json:{}", key);
                    let result: i32 = conn.del(&json_key).await?;
                    debug!("Deleted JSON fallback at {} ({} keys)", json_key, result);
                    Ok(result as u32)
                } else {
                    // Can't support partial deletion in fallback mode
                    warn!(
                        "Partial JSON deletion not supported in fallback mode for {}: {}",
                        key, path
                    );
                    Ok(0)
                }
            }
        }
    }

    /// Merge JSON value at path
    pub async fn json_merge(&self, key: &str, path: &str, value: &Value) -> RedisResult<bool> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;

        // Try RedisJSON module first
        let mut cmd = redis::Cmd::new();
        cmd.arg("JSON.MERGE")
            .arg(key)
            .arg(path)
            .arg(value.to_string());

        match cmd.query_async::<()>(&mut conn).await {
            Ok(_) => {
                debug!("Merged JSON at {}: {}", key, path);
                Ok(true)
            }
            Err(_) => {
                // Fallback: implement merge manually
                if let Ok(existing_value) = self.json_get(key, path).await {
                    if let Some(existing) = existing_value {
                        if let (Value::Object(mut existing_map), Value::Object(merge_map)) =
                            (existing, value.clone())
                        {
                            // Simple merge for objects
                            for (k, v) in merge_map {
                                existing_map.insert(k, v);
                            }
                            return self.json_set(key, path, &Value::Object(existing_map)).await;
                        }
                    }
                }

                // If no existing value or merge failed, just set
                self.json_set(key, path, value).await
            }
        }
    }

    /// Check if JSON path exists
    pub async fn json_exists(&self, key: &str, path: &str) -> RedisResult<bool> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;

        // Try RedisJSON module first
        let mut cmd = redis::Cmd::new();
        cmd.arg("JSON.EXISTS").arg(key).arg(path);

        match cmd.query_async::<u32>(&mut conn).await {
            Ok(result) => Ok(result > 0),
            Err(_) => {
                // Fallback: check if key exists
                let json_key = format!("json:{}", key);
                let exists: bool = conn.exists(&json_key).await?;
                Ok(exists)
            }
        }
    }

    /// Get JSON value type at path
    pub async fn json_type(&self, key: &str, path: &str) -> RedisResult<Option<String>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;

        // Try RedisJSON module first
        let mut cmd = redis::Cmd::new();
        cmd.arg("JSON.TYPE").arg(key).arg(path);

        match cmd.query_async::<Option<String>>(&mut conn).await {
            Ok(result) => Ok(result),
            Err(_) => {
                // Fallback: get the value and infer type
                if let Ok(Some(value)) = self.json_get(key, path).await {
                    let type_str = match value {
                        Value::Null => "null".to_string(),
                        Value::Bool(_) => "boolean".to_string(),
                        Value::Number(_) => "number".to_string(),
                        Value::String(_) => "string".to_string(),
                        Value::Array(_) => "array".to_string(),
                        Value::Object(_) => "object".to_string(),
                    };
                    Ok(Some(type_str))
                } else {
                    Ok(None)
                }
            }
        }
    }

    /// Get JSON array length at path
    pub async fn json_arr_len(&self, key: &str, path: &str) -> RedisResult<Option<u32>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;

        // Try RedisJSON module first
        let mut cmd = redis::Cmd::new();
        cmd.arg("JSON.ARRLEN").arg(key).arg(path);

        match cmd.query_async::<Option<u32>>(&mut conn).await {
            Ok(result) => Ok(result),
            Err(_) => {
                // Fallback: get the array and count
                if let Ok(Some(value)) = self.json_get(key, path).await {
                    if let Value::Array(arr) = value {
                        Ok(Some(arr.len() as u32))
                    } else {
                        Ok(None)
                    }
                } else {
                    Ok(None)
                }
            }
        }
    }

    /// Append to JSON array at path
    pub async fn json_arr_append(
        &self,
        key: &str,
        path: &str,
        values: &[Value],
    ) -> RedisResult<u32> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;

        // Try RedisJSON module first
        let mut cmd = redis::Cmd::new();
        cmd.arg("JSON.ARRAPPEND").arg(key).arg(path);

        for value in values {
            cmd.arg(value.to_string());
        }

        match cmd.query_async::<u32>(&mut conn).await {
            Ok(result) => {
                debug!(
                    "Appended {} values to JSON array {}: {}",
                    values.len(),
                    key,
                    path
                );
                Ok(result)
            }
            Err(_) => {
                // Fallback: implement manually
                if let Ok(Some(existing_value)) = self.json_get(key, path).await {
                    if let Value::Array(mut arr) = existing_value {
                        arr.extend(values.iter().cloned());
                        let len = arr.len() as u32;
                        let new_value = Value::Array(arr);
                        self.json_set(key, path, &new_value).await?;
                        return Ok(len);
                    }
                }

                // Create new array
                let new_value = Value::Array(values.to_vec());
                self.json_set(key, path, &new_value).await?;
                Ok(values.len() as u32)
            }
        }
    }

    /// Get JSON object size at path
    pub async fn json_obj_len(&self, key: &str, path: &str) -> RedisResult<Option<u32>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;

        // Try RedisJSON module first
        let mut cmd = redis::Cmd::new();
        cmd.arg("JSON.OBJLEN").arg(key).arg(path);

        match cmd.query_async::<Option<u32>>(&mut conn).await {
            Ok(result) => Ok(result),
            Err(_) => {
                // Fallback: get the object and count
                if let Ok(Some(value)) = self.json_get(key, path).await {
                    if let Value::Object(obj) = value {
                        Ok(Some(obj.len() as u32))
                    } else {
                        Ok(None)
                    }
                } else {
                    Ok(None)
                }
            }
        }
    }

    /// Get JSON object keys at path
    pub async fn json_obj_keys(&self, key: &str, path: &str) -> RedisResult<Option<Vec<String>>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;

        // Try RedisJSON module first
        let mut cmd = redis::Cmd::new();
        cmd.arg("JSON.OBJKEYS").arg(key).arg(path);

        match cmd.query_async::<Option<Vec<String>>>(&mut conn).await {
            Ok(result) => Ok(result),
            Err(_) => {
                // Fallback: get the object and extract keys
                if let Ok(Some(value)) = self.json_get(key, path).await {
                    if let Value::Object(obj) = value {
                        Ok(Some(obj.keys().cloned().collect()))
                    } else {
                        Ok(None)
                    }
                } else {
                    Ok(None)
                }
            }
        }
    }

    /// Increment numeric value at JSON path
    pub async fn json_num_incrby(
        &self,
        key: &str,
        path: &str,
        value: f64,
    ) -> RedisResult<Option<f64>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;

        // Try RedisJSON module first
        let mut cmd = redis::Cmd::new();
        cmd.arg("JSON.NUMINCRBY").arg(key).arg(path).arg(value);

        match cmd.query_async::<Option<f64>>(&mut conn).await {
            Ok(result) => {
                debug!("Incremented JSON number at {} {} by {}", key, path, value);
                Ok(result)
            }
            Err(_) => {
                // Fallback: implement manually
                if let Ok(Some(existing_value)) = self.json_get(key, path).await {
                    if let Value::Number(num) = existing_value {
                        if let Some(current) = num.as_f64() {
                            let new_value = current + value;
                            self.json_set(key, path, &json!(new_value)).await?;
                            return Ok(Some(new_value));
                        }
                    }
                }

                // Set new value
                self.json_set(key, path, &json!(value)).await?;
                Ok(Some(value))
            }
        }
    }

    /// String append to JSON value at path
    pub async fn json_str_append(&self, key: &str, path: &str, value: &str) -> RedisResult<u32> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;

        // Try RedisJSON module first
        let mut cmd = redis::Cmd::new();
        cmd.arg("JSON.STRAPPEND").arg(key).arg(path).arg(value);

        match cmd.query_async::<u32>(&mut conn).await {
            Ok(result) => {
                debug!("Appended '{}' to JSON string at {} {}", value, key, path);
                Ok(result)
            }
            Err(_) => {
                // Fallback: implement manually
                if let Ok(Some(existing_value)) = self.json_get(key, path).await {
                    if let Value::String(mut str_val) = existing_value {
                        str_val.push_str(value);
                        let len = str_val.len() as u32;
                        let new_value = Value::String(str_val);
                        self.json_set(key, path, &new_value).await?;
                        return Ok(len);
                    }
                }

                // Set new string
                self.json_set(key, path, &json!(value)).await?;
                Ok(value.len() as u32)
            }
        }
    }

    /// Clear JSON values at path (sets to null)
    pub async fn json_clear(&self, key: &str, path: &str) -> RedisResult<u32> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;

        // Try RedisJSON module first
        let mut cmd = redis::Cmd::new();
        cmd.arg("JSON.CLEAR").arg(key).arg(path);

        match cmd.query_async::<u32>(&mut conn).await {
            Ok(result) => {
                debug!("Cleared JSON at {}: {} ({} paths)", key, path, result);
                Ok(result)
            }
            Err(_) => {
                // Fallback: set to null
                self.json_set(key, path, &json!(null)).await?;
                Ok(1)
            }
        }
    }

    /// Delete JSON key completely
    pub async fn delete_json_key(&self, key: &str) -> RedisResult<bool> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;

        // Try JSON.DELETE first
        let result = self.json_del(key, "$").await?;

        if result == 0 {
            // Fallback: delete regular key
            let json_key = format!("json:{}", key);
            let deleted: i32 = conn.del(&json_key).await?;
            Ok(deleted > 0)
        } else {
            Ok(true)
        }
    }

    /// List all JSON keys matching pattern
    pub async fn list_json_keys(&self, pattern: &str) -> RedisResult<Vec<String>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;

        // Get both JSON keys and fallback keys
        let json_pattern = format!("json:*{}", pattern);
        let keys: Vec<String> = conn.keys(pattern).await?;
        let fallback_keys: Vec<String> = conn.keys(&json_pattern).await?;

        // Remove json: prefix from fallback keys and merge
        let mut all_keys = keys;
        for fallback_key in fallback_keys {
            if let Some(clean_key) = fallback_key.strip_prefix("json:") {
                if !all_keys.contains(&clean_key.to_string()) {
                    all_keys.push(clean_key.to_string());
                }
            }
        }

        debug!(
            "Found {} JSON keys matching pattern: {}",
            all_keys.len(),
            pattern
        );
        Ok(all_keys)
    }

    /// Validate JSON syntax
    pub fn validate_json(&self, json_str: &str) -> Result<Value, serde_json::Error> {
        serde_json::from_str(json_str)
    }

    /// Pretty print JSON
    pub fn pretty_print_json(&self, value: &Value) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(value)
    }
}

/// Pre-configured JSON service for GridTokenX
pub struct GridTokenXJSONService {
    service: RedisJSONService,
}

impl GridTokenXJSONService {
    /// Create GridTokenX JSON service
    pub fn new(redis_url: &str) -> RedisResult<Self> {
        let service = RedisJSONService::new(redis_url)?;
        Ok(Self { service })
    }

    /// Store user preferences
    pub async fn store_user_preferences(
        &self,
        user_id: &str,
        preferences: &Value,
    ) -> RedisResult<bool> {
        let key = format!("user_preferences:{}", user_id);
        self.service.json_set(&key, "$", preferences).await
    }

    /// Get user preferences
    pub async fn get_user_preferences(&self, user_id: &str) -> RedisResult<Option<Value>> {
        let key = format!("user_preferences:{}", user_id);
        self.service.json_get(&key, "$").await
    }

    /// Update specific user preference
    pub async fn update_user_preference(
        &self,
        user_id: &str,
        path: &str,
        value: &Value,
    ) -> RedisResult<bool> {
        let key = format!("user_preferences:{}", user_id);
        let full_path = format!("$.{}", path);
        self.service.json_set(&key, &full_path, value).await
    }

    /// Store trading configuration
    pub async fn store_trading_config(&self, config_id: &str, config: &Value) -> RedisResult<bool> {
        let key = format!("trading_config:{}", config_id);
        self.service.json_set(&key, "$", config).await
    }

    /// Get trading configuration
    pub async fn get_trading_config(&self, config_id: &str) -> RedisResult<Option<Value>> {
        let key = format!("trading_config:{}", config_id);
        self.service.json_get(&key, "$").await
    }

    /// Store market configuration
    pub async fn store_market_config(&self, market_id: &str, config: &Value) -> RedisResult<bool> {
        let key = format!("market_config:{}", market_id);
        self.service.json_set(&key, "$", config).await
    }

    /// Get market configuration
    pub async fn get_market_config(&self, market_id: &str) -> RedisResult<Option<Value>> {
        let key = format!("market_config:{}", market_id);
        self.service.json_get(&key, "$").await
    }

    /// Store blockchain configuration
    pub async fn store_blockchain_config(
        &self,
        network: &str,
        config: &Value,
    ) -> RedisResult<bool> {
        let key = format!("blockchain_config:{}", network);
        self.service.json_set(&key, "$", config).await
    }

    /// Get blockchain configuration
    pub async fn get_blockchain_config(&self, network: &str) -> RedisResult<Option<Value>> {
        let key = format!("blockchain_config:{}", network);
        self.service.json_get(&key, "$").await
    }

    /// Store dynamic form data
    pub async fn store_form_data(&self, form_id: &str, data: &Value) -> RedisResult<bool> {
        let key = format!("form_data:{}", form_id);
        self.service.json_set(&key, "$", data).await
    }

    /// Get dynamic form data
    pub async fn get_form_data(&self, form_id: &str) -> RedisResult<Option<Value>> {
        let key = format!("form_data:{}", form_id);
        self.service.json_get(&key, "$").await
    }

    /// Append to user activity log
    pub async fn append_user_activity(&self, user_id: &str, activity: &Value) -> RedisResult<u32> {
        let key = format!("user_activity:{}", user_id);
        let path = "$.activities";
        self.service
            .json_arr_append(&key, path, &[activity.clone()])
            .await
    }

    /// Get user activity log
    pub async fn get_user_activity(
        &self,
        user_id: &str,
        limit: Option<usize>,
    ) -> RedisResult<Option<Value>> {
        let key = format!("user_activity:{}", user_id);
        let activities = self.service.json_get(&key, "$.activities").await?;

        if let Some(mut activities_array) = activities {
            if let Value::Array(ref mut arr) = activities_array {
                if let Some(limit_val) = limit {
                    arr.truncate(limit_val);
                }
                // Reverse to show most recent first
                arr.reverse();
            }
            Ok(Some(activities_array))
        } else {
            Ok(None)
        }
    }

    /// Store trading analytics
    pub async fn store_trading_analytics(
        &self,
        analytics_id: &str,
        data: &Value,
    ) -> RedisResult<bool> {
        let key = format!("trading_analytics:{}", analytics_id);
        self.service.json_set(&key, "$", data).await
    }

    /// Get trading analytics
    pub async fn get_trading_analytics(&self, analytics_id: &str) -> RedisResult<Option<Value>> {
        let key = format!("trading_analytics:{}", analytics_id);
        self.service.json_get(&key, "$").await
    }

    /// Update analytics metrics
    pub async fn update_analytics_metric(
        &self,
        analytics_id: &str,
        metric_path: &str,
        value: f64,
    ) -> RedisResult<Option<f64>> {
        let key = format!("trading_analytics:{}", analytics_id);
        let full_path = format!("$.metrics.{}", metric_path);
        self.service.json_num_incrby(&key, &full_path, value).await
    }

    /// Store system configuration
    pub async fn store_system_config(&self, config_key: &str, config: &Value) -> RedisResult<bool> {
        let key = format!("system_config:{}", config_key);
        self.service.json_set(&key, "$", config).await
    }

    /// Get system configuration
    pub async fn get_system_config(&self, config_key: &str) -> RedisResult<Option<Value>> {
        let key = format!("system_config:{}", config_key);
        self.service.json_get(&key, "$").await
    }

    /// Validate and store configuration
    pub async fn validate_and_store_config(
        &self,
        config_type: &str,
        config_id: &str,
        config: &Value,
    ) -> RedisResult<bool> {
        // Basic validation
        if config.is_null() {
            warn!(
                "Attempted to store null config for {}: {}",
                config_type, config_id
            );
            return Ok(false);
        }

        // Validate required fields based on config type
        match config_type {
            "trading" => {
                if !config.get("market_id").is_some() || !config.get("rules").is_some() {
                    warn!("Invalid trading config: missing required fields");
                    return Ok(false);
                }
            }
            "market" => {
                if !config.get("symbol").is_some() || !config.get("base_currency").is_some() {
                    warn!("Invalid market config: missing required fields");
                    return Ok(false);
                }
            }
            "blockchain" => {
                if !config.get("network").is_some() || !config.get("rpc_url").is_some() {
                    warn!("Invalid blockchain config: missing required fields");
                    return Ok(false);
                }
            }
            _ => {}
        }

        // Store the validated configuration
        let key = format!("{}_config:{}", config_type, config_id);
        let result = self.service.json_set(&key, "$", config).await?;

        if result {
            info!("Validated and stored {} config: {}", config_type, config_id);
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_validation() {
        let service = RedisJSONService::new("redis://localhost").unwrap();

        let valid_json = json!({"test": "value", "number": 42});
        assert!(service.validate_json(&valid_json.to_string()).is_ok());

        let invalid_json = "{invalid json}";
        assert!(service.validate_json(invalid_json).is_err());
    }

    #[test]
    fn test_pretty_print_json() {
        let service = RedisJSONService::new("redis://localhost").unwrap();

        let json_value = json!({"test": "value", "number": 42});
        let pretty = service.pretty_print_json(&json_value).unwrap();

        assert!(pretty.contains("test"));
        assert!(pretty.contains("value"));
        assert!(pretty.contains("number"));
        assert!(pretty.contains("42"));
    }

    #[test]
    fn test_time_point_creation() {
        use crate::services::redis::timeseries::TimeSeriesPoint;

        let point = TimeSeriesPoint::new(1609459200000, 100.5);
        assert_eq!(point.timestamp, 1609459200000);
        assert_eq!(point.value, 100.5);
    }
}
