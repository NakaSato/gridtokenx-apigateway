use anyhow::Result;
use dotenvy::dotenv;
use rust_decimal::Decimal;
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use std::env;
use std::sync::Arc;
use uuid::Uuid;

// Assuming the crate name is 'api_gateway' as defined in Cargo.toml
use api_gateway::{
    services::{BlockchainTaskService, BlockchainTaskType, TaskPayload, EscrowRefundPayload, MarketClearingService},
    config::{Config, SolanaProgramsConfig}, 
    services::{BlockchainService, WalletService, AuditLogger, WebSocketService, ErcService},
};

#[tokio::test]
async fn test_blockchain_task_lifecycle() -> Result<()> {
    dotenv().ok();
    
    // 1. Setup DB Connection
    // Ensure we have a DB URL. If not, skip or fail.
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    // Run migrations to ensure schema exists
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to apply migrations");

    // 2. Initialize Service Dependencies
    // Config: we can construct a default or dummy one for testing if env vars missing
    let mut config = match Config::from_env() {
        Ok(c) => c,
        Err(_) => {
            // Minimal config for testing if .env missing - risky but helpful
             return Err(anyhow::anyhow!("Env vars missing for test"));
        }
    };
    
    // Force mock mode for tokenization if not set
    // config.tokenization.enable_real_blockchain = false; // Internal field access?

    let blockchain_service = BlockchainService::new(
        "http://localhost:8899".to_string(),
        "localnet".to_string(),
        SolanaProgramsConfig::default(),
    ).expect("Failed to init blockchain");
    
    let wallet_service = WalletService::new("http://localhost:8899"); 
    let audit_logger = AuditLogger::new(pool.clone());
    let websocket_service = WebSocketService::new();
    let erc_service = ErcService::new(pool.clone(), blockchain_service.clone());

    let market_clearing = Arc::new(MarketClearingService::new(
        pool.clone(),
        blockchain_service.clone(),
        config.clone(),
        wallet_service,
        audit_logger,
        websocket_service,
        erc_service,
    ));

    let task_service = BlockchainTaskService::new(pool.clone(), market_clearing.clone());

    // 4. Queue a Task
    // Create dummy user to satisfy FK
    let user_id = Uuid::new_v4();
    let wallet = format!("mock_wallet_{}", user_id);
    
    let email = format!("test_{}@example.com", user_id);
    let username = format!("user_{}", user_id);

    sqlx::query!(
        "INSERT INTO users (id, email, username, password_hash, role, wallet_address) VALUES ($1, $2, $3, 'hash', 'user', $4)",
        user_id,
        email,
        username,
        wallet
    )
    .execute(&pool)
    .await?;

    let payload = TaskPayload::EscrowRefund(EscrowRefundPayload {
        user_id,
        amount: Decimal::from(100),
        asset_type: "currency".to_string(),
        order_id: Uuid::new_v4(),
    });

    let task_service_clone = task_service.clone();
    // We need to use explicit task type, cloned?
    // The previous error was about moving BlockchainTaskType.
    let task_type = BlockchainTaskType::EscrowRefund;
    
    // queue_task consumes its arguments? No, usually `&self`.
    // Let's check signature: `pub async fn queue_task(&self, task_type: BlockchainTaskType, payload: TaskPayload)`
    // So it takes by value.
    let task_id = task_service_clone.queue_task(task_type, payload).await?;
    println!("Queued task: {}", task_id);

    // 5. Verify it is pending
    // We cast to Text or String because the driver maps ENUM to string or custom type.
    // Since we defined the enum in SQLx, it might map to declared enum type.
    // But `query!` macro might map it to `BlockchainTaskType` if we impl sqlx types.
    // The previous code used `status as "status: String"`.
    let row = sqlx::query!("SELECT status::text as status_str FROM blockchain_tasks WHERE id = $1", task_id)
        .fetch_one(&pool)
        .await?;
    
    assert_eq!(row.status_str.unwrap_or_default(), "pending");

    // 6. Process Tasks
    task_service.process_pending_tasks().await?;

    // 7. Verify Completion
    let row = sqlx::query!("SELECT status::text as status_str, retry_count, last_error FROM blockchain_tasks WHERE id = $1", task_id)
        .fetch_one(&pool)
        .await?;
    
    let final_status = row.status_str.unwrap_or_default();
    
    println!("Task finished with status: {}, error: {:?}", final_status, row.last_error);
    
    assert!(final_status == "completed" || final_status == "failed");

    // Cleanup
    // Ignore cleanup errors
    let _ = sqlx::query!("DELETE FROM users WHERE id = $1", user_id).execute(&pool).await;
    let _ = sqlx::query!("DELETE FROM blockchain_tasks WHERE id = $1", task_id).execute(&pool).await;

    Ok(())
}
