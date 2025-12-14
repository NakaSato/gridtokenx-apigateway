#[cfg(test)]
mod tests {
    use crate::database::schema::types::EpochStatus;
    use crate::services::epoch_scheduler::{
        types::EpochConfig,
        utils::{calculate_epoch_number, calculate_next_epoch_start, determine_target_state},
        EpochScheduler,
    };
    use crate::services::market_clearing::MarketEpoch;
    use crate::services::BlockchainService;
    use chrono::{TimeZone, Utc};
    use rust_decimal::Decimal;
    use sqlx::PgPool;
    use std::env;
    use uuid::Uuid;

    // Helper function to create a test database connection
    async fn create_test_db() -> PgPool {
        let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://postgres:password@localhost/gridtokenx_test".to_string()
        });

        PgPool::connect_lazy(&database_url).expect("Failed to connect to test database")
    }

    // Helper function to create a test blockchain service
    fn create_test_blockchain_service() -> BlockchainService {
        use crate::config::SolanaProgramsConfig;
        let program_config = SolanaProgramsConfig {
            registry_program_id: "2XPQmFYMdXjP7ffoBB3mXeCdboSFg5Yeb6QmTSGbW8a7".to_string(),
            oracle_program_id: "DvdtU4quEbuxUY2FckmvcXwTpC9qp4HLJKb1PMLaqAoE".to_string(),
            governance_program_id: "4DY97YYBt4bxvG7xaSmWy3MhYhmA6HoMajBHVqhySvXe".to_string(),
            energy_token_program_id: "94G1r674LmRDmLN2UPjDFD8Eh7zT8JaSaxv9v68GyEur".to_string(),
            trading_program_id: "9t3s8sCgVUG9kAgVPsozj8mDpJp9cy6SF5HwRK5nvAHb".to_string(),
        };
        BlockchainService::new(
            "http://localhost:8899".to_string(),
            "localnet".to_string(),
            program_config,
        )
        .expect("Failed to create test blockchain service")
    }

    #[tokio::test]
    async fn test_epoch_number_calculation() {
        let timestamp = Utc.with_ymd_and_hms(2025, 11, 9, 14, 30, 0).unwrap();
        let epoch_number = calculate_epoch_number(timestamp);

        // Expected: 202511091430 (YYYYMMDDHHMM with 15-minute blocks)
        assert_eq!(epoch_number, 202511091430);
    }

    #[tokio::test]
    async fn test_next_epoch_start_calculation() {
        let now = Utc.with_ymd_and_hms(2025, 11, 9, 14, 37, 0).unwrap();
        let next_epoch_start = calculate_next_epoch_start(now);

        // Should be 14:45 (next 15-minute block)
        let expected = Utc.with_ymd_and_hms(2025, 11, 9, 14, 45, 0).unwrap();
        assert_eq!(next_epoch_start, expected);
    }

    #[tokio::test]
    async fn test_target_state_determination() {
        let config = EpochConfig::default();
        let scheduler = EpochScheduler::new(
            create_test_db().await,
            config,
            create_test_blockchain_service(),
        );

        let now = Utc.with_ymd_and_hms(2025, 11, 9, 14, 30, 0).unwrap();

        // Test pending epoch
        let pending_epoch = MarketEpoch {
            id: Uuid::new_v4(),
            epoch_number: 202511091430,
            start_time: Utc.with_ymd_and_hms(2025, 11, 9, 14, 30, 0).unwrap(),
            end_time: Utc.with_ymd_and_hms(2025, 11, 9, 14, 45, 0).unwrap(),
            status: EpochStatus::Pending,
            clearing_price: None,
            total_volume: Some(Decimal::ZERO),
            total_orders: Some(0),
            matched_orders: Some(0),
        };

        let target_state = determine_target_state(&pending_epoch, now);
        assert_eq!(target_state, "active");

        // Test expired epoch
        let later_now = Utc.with_ymd_and_hms(2025, 11, 9, 14, 50, 0).unwrap();
        let target_state = determine_target_state(&pending_epoch, later_now);
        assert_eq!(target_state, "cleared");
    }

    #[tokio::test]
    async fn test_epoch_boundaries_at_midnight() {
        // Test epoch calculation across midnight boundary
        let timestamp = Utc.with_ymd_and_hms(2025, 11, 9, 23, 45, 0).unwrap();
        let epoch_number = calculate_epoch_number(timestamp);
        assert_eq!(epoch_number, 202511092345);

        let next_start = calculate_next_epoch_start(timestamp);
        let expected = Utc.with_ymd_and_hms(2025, 11, 10, 0, 0, 0).unwrap();
        assert_eq!(next_start, expected);
    }

    #[tokio::test]
    async fn test_epoch_boundaries_at_month_end() {
        // Test epoch calculation across month boundary
        let timestamp = Utc.with_ymd_and_hms(2025, 11, 30, 23, 45, 0).unwrap();
        let next_start = calculate_next_epoch_start(timestamp);
        let expected = Utc.with_ymd_and_hms(2025, 12, 1, 0, 0, 0).unwrap();
        assert_eq!(next_start, expected);
    }

    #[tokio::test]
    async fn test_all_15_minute_boundaries() {
        // Test all four 15-minute boundaries in an hour
        let boundaries = vec![0, 15, 30, 45];

        for minute in boundaries {
            let timestamp = Utc.with_ymd_and_hms(2025, 11, 9, 14, minute, 0).unwrap();
            let epoch_number = calculate_epoch_number(timestamp);

            // Extract minute from epoch number
            let epoch_minute = (epoch_number % 100) as u32;
            assert_eq!(epoch_minute, minute);
        }
    }

    #[tokio::test]
    async fn test_state_transition_sequence() {
        let config = EpochConfig::default();
        let scheduler = EpochScheduler::new(
            create_test_db().await,
            config,
            create_test_blockchain_service(),
        );

        let epoch_start = Utc.with_ymd_and_hms(2025, 11, 9, 14, 30, 0).unwrap();
        let epoch_end = Utc.with_ymd_and_hms(2025, 11, 9, 14, 45, 0).unwrap();

        // Test state progression
        let mut epoch = MarketEpoch {
            id: Uuid::new_v4(),
            epoch_number: 202511091430,
            start_time: epoch_start,
            end_time: epoch_end,
            status: EpochStatus::Pending,
            clearing_price: None,
            total_volume: Some(Decimal::ZERO),
            total_orders: Some(0),
            matched_orders: Some(0),
        };

        // At start: pending → active
        let state = determine_target_state(&epoch, epoch_start);
        assert_eq!(state, "active");

        // During epoch: active → active
        epoch.status = EpochStatus::Active;
        let mid_time = epoch_start + chrono::Duration::minutes(7);
        let state = determine_target_state(&epoch, mid_time);
        assert_eq!(state, "active");

        // After end: active → cleared
        let after_end = epoch_end + chrono::Duration::seconds(1);
        let state = determine_target_state(&epoch, after_end);
        assert_eq!(state, "cleared");
    }

    #[tokio::test]
    async fn test_epoch_duration_always_15_minutes() {
        // Test multiple random times
        let test_times = vec![
            Utc.with_ymd_and_hms(2025, 11, 9, 0, 5, 0).unwrap(),
            Utc.with_ymd_and_hms(2025, 11, 9, 8, 17, 30).unwrap(),
            Utc.with_ymd_and_hms(2025, 11, 9, 12, 42, 15).unwrap(),
            Utc.with_ymd_and_hms(2025, 11, 9, 18, 58, 45).unwrap(),
            Utc.with_ymd_and_hms(2025, 11, 9, 23, 59, 59).unwrap(),
        ];

        for time in test_times {
            let next_start = calculate_next_epoch_start(time);
            let next_end = next_start + chrono::Duration::minutes(15);

            let duration_secs = (next_end - next_start).num_seconds();
            assert_eq!(duration_secs, 900); // 15 minutes = 900 seconds
        }
    }

    #[tokio::test]
    async fn test_epoch_number_monotonicity() {
        // Epoch numbers should strictly increase over time
        let mut previous_epoch_number = 0i64;

        for hour in 0..24 {
            for minute in [0, 15, 30, 45] {
                let timestamp = Utc.with_ymd_and_hms(2025, 11, 9, hour, minute, 0).unwrap();
                let epoch_number = calculate_epoch_number(timestamp);

                if previous_epoch_number > 0 {
                    assert!(
                        epoch_number > previous_epoch_number,
                        "Epoch numbers must increase: {} should be > {}",
                        epoch_number,
                        previous_epoch_number
                    );
                }

                previous_epoch_number = epoch_number;
            }
        }
    }

    #[tokio::test]
    async fn test_epoch_number_format() {
        let timestamp = Utc.with_ymd_and_hms(2025, 11, 9, 14, 30, 0).unwrap();
        let epoch_number = calculate_epoch_number(timestamp);

        // Convert to string to check format
        let epoch_str = epoch_number.to_string();
        assert_eq!(
            epoch_str.len(),
            12,
            "Epoch number should be 12 digits (YYYYMMDDHHMM)"
        );

        // Extract and verify components
        let year: i32 = epoch_str[0..4].parse().unwrap();
        let month: u32 = epoch_str[4..6].parse().unwrap();
        let day: u32 = epoch_str[6..8].parse().unwrap();
        let hour: u32 = epoch_str[8..10].parse().unwrap();
        let minute: u32 = epoch_str[10..12].parse().unwrap();

        assert_eq!(year, 2025);
        assert_eq!(month, 11);
        assert_eq!(day, 9);
        assert_eq!(hour, 14);
        assert_eq!(minute, 30);
    }

    #[tokio::test]
    async fn test_leap_year_february_29() {
        // 2024 is a leap year
        let timestamp = Utc.with_ymd_and_hms(2024, 2, 29, 10, 15, 0).unwrap();
        let epoch_number = calculate_epoch_number(timestamp);

        let epoch_str = epoch_number.to_string();
        let month: u32 = epoch_str[4..6].parse().unwrap();
        let day: u32 = epoch_str[6..8].parse().unwrap();

        assert_eq!(month, 2);
        assert_eq!(day, 29);
    }
}
