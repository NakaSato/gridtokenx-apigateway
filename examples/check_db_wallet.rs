use sqlx::postgres::PgPoolOptions;
use dotenvy::dotenv;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();
    let database_url = env::var("DATABASE_URL")?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    let username = env::args().nth(1).unwrap_or_else(|| "seller_1767412251".to_string());
    println!("Querying data for user: {}", username);

    let row = sqlx::query!(
        "SELECT id, username, wallet_address, encrypted_private_key IS NOT NULL as has_key, wallet_salt IS NOT NULL as has_salt, encryption_iv IS NOT NULL as has_iv FROM users WHERE username = $1",
        username
    )
    .fetch_one(&pool)
    .await?;

    println!("ID: {}", row.id);
    println!("Username: {}", row.username);
    println!("wallet_address: {:?}", row.wallet_address);
    println!("Has Key: {:?}", row.has_key);
    println!("Has Salt: {:?}", row.has_salt);
    println!("Has IV: {:?}", row.has_iv);

    Ok(())
}
