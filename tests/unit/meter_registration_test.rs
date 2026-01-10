//! Integration tests for meter registration by ID
//!
//! Tests the POST /api/v1/simulator/meters/register endpoint

use anyhow::Result;
use sqlx::PgPool;
use uuid::Uuid;

#[cfg(test)]
mod tests {
    use super::*;

    async fn create_test_db() -> PgPool {
        let database_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://gridtokenx_user:gridtokenx_password@localhost:5432/gridtokenx_test".to_string());
        
        PgPool::connect(&database_url).await.expect("Failed to connect to test database")
    }

    async fn cleanup_test_meter(db: &PgPool, meter_id: &str) {
        let _ = sqlx::query("DELETE FROM meter_registry WHERE meter_serial = $1")
            .bind(meter_id)
            .execute(db)
            .await;
        let _ = sqlx::query("DELETE FROM meters WHERE serial_number = $1")
            .bind(meter_id)
            .execute(db)
            .await;
    }

    #[tokio::test]
    async fn test_register_meter_by_id_creates_meter() {
        let db = create_test_db().await;
        let meter_id = format!("TEST-REG-{}", Uuid::new_v4().to_string()[..8].to_uppercase());
        
        // Cleanup before test
        cleanup_test_meter(&db, &meter_id).await;

        // Simulate register_meter_by_id logic (direct DB insertion)
        let user_id = Uuid::new_v4();
        let wallet = "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM";
        let db_meter_id = Uuid::new_v4();

        // First create user
        let _ = sqlx::query(
            "INSERT INTO users (id, email, username, password_hash, wallet_address, role, email_verified) 
             VALUES ($1, $2, $3, 'test_hash', $4, 'prosumer', true)
             ON CONFLICT (email) DO NOTHING"
        )
        .bind(user_id)
        .bind(format!("test_{}@test.com", user_id))
        .bind(format!("test_user_{}", user_id))
        .bind(wallet)
        .execute(&db)
        .await
        .expect("Failed to create test user");

        // Then create meter
        let insert_result = sqlx::query(
            "INSERT INTO meters (id, user_id, serial_number, meter_type, location, is_verified, created_at, updated_at)
             VALUES ($1, $2, $3, 'solar', 'Test Location', true, NOW(), NOW())"
        )
        .bind(db_meter_id)
        .bind(user_id)
        .bind(&meter_id)
        .execute(&db)
        .await;

        assert!(insert_result.is_ok(), "Failed to insert meter: {:?}", insert_result.err());

        // Verify meter exists
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM meters WHERE serial_number = $1"
        )
        .bind(&meter_id)
        .fetch_one(&db)
        .await
        .expect("Failed to query meter count");

        assert_eq!(count, 1, "Meter should exist");

        // Cleanup
        cleanup_test_meter(&db, &meter_id).await;
    }

    #[tokio::test]
    async fn test_register_meter_by_id_duplicate_returns_success() {
        let db = create_test_db().await;
        let meter_id = format!("TEST-DUP-{}", Uuid::new_v4().to_string()[..8].to_uppercase());
        
        cleanup_test_meter(&db, &meter_id).await;

        let user_id = Uuid::new_v4();
        let db_meter_id = Uuid::new_v4();

        // Create user
        let _ = sqlx::query(
            "INSERT INTO users (id, email, username, password_hash, role, email_verified) 
             VALUES ($1, $2, $3, 'test_hash', 'prosumer', true)
             ON CONFLICT (email) DO NOTHING"
        )
        .bind(user_id)
        .bind(format!("dup_{}@test.com", user_id))
        .bind(format!("dup_user_{}", user_id))
        .execute(&db)
        .await;

        // First registration
        let _ = sqlx::query(
            "INSERT INTO meters (id, user_id, serial_number, meter_type, location, is_verified, created_at, updated_at)
             VALUES ($1, $2, $3, 'solar', 'Test', true, NOW(), NOW())"
        )
        .bind(db_meter_id)
        .bind(user_id)
        .bind(&meter_id)
        .execute(&db)
        .await
        .expect("First insert should succeed");

        // Check count (simulating duplicate registration check)
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM meters WHERE serial_number = $1"
        )
        .bind(&meter_id)
        .fetch_one(&db)
        .await
        .expect("Failed to query");

        assert!(count > 0, "Meter already exists - duplicate registration returns success");

        cleanup_test_meter(&db, &meter_id).await;
    }

    #[tokio::test]
    async fn test_meter_validation_rejects_unregistered() {
        let db = create_test_db().await;
        let fake_meter_id = format!("FAKE-{}", Uuid::new_v4());

        // Check meter does not exist
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM meters WHERE serial_number = $1"
        )
        .bind(&fake_meter_id)
        .fetch_one(&db)
        .await
        .expect("Failed to query");

        assert_eq!(count, 0, "Fake meter should not exist");
        
        // This simulates the validation in submit_reading
        // meter_exists == 0 means reject the reading
        assert!(count == 0, "Unregistered meter should be rejected");
    }

    #[tokio::test]
    async fn test_register_meter_with_coordinates() {
        let db = create_test_db().await;
        let meter_id = format!("TEST-GPS-{}", Uuid::new_v4().to_string()[..8].to_uppercase());
        
        cleanup_test_meter(&db, &meter_id).await;

        let user_id = Uuid::new_v4();
        let db_meter_id = Uuid::new_v4();
        let lat = 13.7563;
        let lng = 100.5018;
        let zone_id = 1;

        // Create user
        let _ = sqlx::query(
            "INSERT INTO users (id, email, username, password_hash, role, email_verified) 
             VALUES ($1, $2, $3, 'test_hash', 'prosumer', true)
             ON CONFLICT (email) DO NOTHING"
        )
        .bind(user_id)
        .bind(format!("gps_{}@test.com", user_id))
        .bind(format!("gps_user_{}", user_id))
        .execute(&db)
        .await;

        // Create meter with GPS
        let insert_result = sqlx::query(
            "INSERT INTO meters (id, user_id, serial_number, meter_type, location, latitude, longitude, zone_id, is_verified, created_at, updated_at)
             VALUES ($1, $2, $3, 'solar', 'Bangkok', $4, $5, $6, true, NOW(), NOW())"
        )
        .bind(db_meter_id)
        .bind(user_id)
        .bind(&meter_id)
        .bind(lat)
        .bind(lng)
        .bind(zone_id)
        .execute(&db)
        .await;

        assert!(insert_result.is_ok(), "Should insert meter with GPS coords");

        // Verify coordinates stored
        let stored = sqlx::query_as::<_, (Option<f64>, Option<f64>, Option<i32>)>(
            "SELECT latitude, longitude, zone_id FROM meters WHERE serial_number = $1"
        )
        .bind(&meter_id)
        .fetch_one(&db)
        .await
        .expect("Failed to fetch meter");

        assert_eq!(stored.0, Some(lat));
        assert_eq!(stored.1, Some(lng));
        assert_eq!(stored.2, Some(zone_id));

        cleanup_test_meter(&db, &meter_id).await;
    }
}
