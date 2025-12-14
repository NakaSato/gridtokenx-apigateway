use utoipa::OpenApi;
use utoipa::openapi::security::{SecurityScheme, HttpAuthScheme, HttpBuilder};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "GridTokenX Platform API",
        version = "0.1.1",
        description = "API Gateway for GridTokenX Platform",
        contact(
            name = "Engineering Department",
            email = "wit.chanthawat@gmail.com"
        ),
        license(
            name = "MIT"
        )
    ),
    servers(
        (url = "http://localhost:8080", description = "Local development server"),
        (url = "https://api.gridtokenx.com", description = "Production server")
    ),
    paths(
        // Health
        crate::handlers::health::health_check,
        
        // Authentication
        crate::handlers::auth::login,
        crate::handlers::auth::get_profile,
        crate::handlers::auth::update_profile,
        crate::handlers::auth::change_password,
        crate::handlers::auth::get_user,
        crate::handlers::auth::list_users,
        
        // User Management
        crate::handlers::user::register,
        crate::handlers::user::update_wallet_address,
        crate::handlers::user::remove_wallet_address,
        crate::handlers::user::get_my_activity,
        crate::handlers::user::get_user_activity,
        crate::handlers::user::admin_update_user,
        crate::handlers::user::admin_deactivate_user,
        crate::handlers::user::admin_reactivate_user,
        
        // Email Verification
        crate::handlers::email_verification::verify_email,
        crate::handlers::email_verification::resend_verification,
        
        // Wallet Authentication
        crate::handlers::wallet_auth::register_with_wallet,
        crate::handlers::wallet_auth::login_with_wallet,
        
        // Trading Operations
        crate::handlers::trading::orders::create::create_order,
        crate::handlers::trading::orders::queries::get_user_orders,
        crate::handlers::trading::get_market_data,
        crate::handlers::trading::get_trading_stats,
        crate::handlers::trading::get_blockchain_market_data,
        crate::handlers::trading::create_blockchain_order,
        crate::handlers::trading::match_blockchain_orders,
        
        // Blockchain Integration
        crate::handlers::blockchain::submit_transaction,
        crate::handlers::blockchain::get_transaction_history,
        crate::handlers::blockchain::get_transaction_status,
        crate::handlers::blockchain::interact_with_program,
        crate::handlers::blockchain::get_account_info,
        crate::handlers::blockchain::get_network_status,
        
        // Smart Meters
        crate::handlers::meter::submit_reading,
        crate::handlers::meter::get_my_readings,
        crate::handlers::meter::get_readings_by_wallet,
        crate::handlers::meter::get_unminted_readings,
        crate::handlers::meter::mint_from_reading,
        crate::handlers::meter::get_user_stats,
        
        // Token Operations
        crate::handlers::token::get_token_balance,
        crate::handlers::token::get_token_info,
        crate::handlers::token::mint_tokens,
        crate::handlers::token::mint_from_reading,
        
        // Registry
        crate::handlers::registry::get_blockchain_user,
        crate::handlers::registry::update_user_role,
        
        // Governance
        crate::handlers::governance::get_governance_status,
        crate::handlers::governance::emergency_pause,
        crate::handlers::governance::emergency_unpause,
        
        // Oracle
        crate::handlers::oracle::submit_price,
        crate::handlers::oracle::get_current_prices,
        crate::handlers::oracle::get_oracle_data,
        
        // ERC (Energy Renewable Certificates)
        crate::handlers::erc::issue_certificate,
        crate::handlers::erc::get_certificate,
        crate::handlers::erc::get_certificates_by_wallet,
        crate::handlers::erc::get_my_certificates,
        crate::handlers::erc::get_my_certificate_stats,
        crate::handlers::erc::retire_certificate,
        
        // Blockchain Test
        crate::handlers::blockchain_test::create_test_transaction,
        crate::handlers::blockchain_test::get_test_transaction_status,
        crate::handlers::blockchain_test::get_test_statistics,
        
        // WebSocket
        crate::handlers::websocket::websocket_handler,
        crate::handlers::websocket::websocket_stats,
        
        // Admin - Market
        crate::handlers::admin::get_market_health,
        crate::handlers::admin::get_trading_analytics,
        crate::handlers::admin::market_control,
    ),
    components(schemas(
        // Common response types
        crate::handlers::health::HealthResponse,
        crate::handlers::health::HealthStatus,
        crate::handlers::health::ServiceHealth,
        
        // Auth types
        crate::auth::Claims,
        crate::auth::SecureAuthResponse,
        crate::auth::UserInfo,
        crate::auth::SecureUserInfo,
        crate::handlers::auth::LoginRequest,
        crate::handlers::auth::UpdateProfileRequest,
        crate::handlers::auth::ChangePasswordRequest,
        crate::handlers::auth::UserSearchQuery,
        crate::handlers::auth::UserListResponse,
        
        // User Management types
        crate::handlers::user::RegisterRequest,
        crate::handlers::user::UpdateWalletRequest,
        crate::handlers::user::AdminUpdateUserRequest,
        crate::handlers::user::UserActivity,
        crate::handlers::user::UserActivityResponse,
        crate::handlers::user::RegisterResponse,
        crate::handlers::user::ActivityQuery,
        crate::handlers::user::ActivityListResponse,
        
        // Email Verification types
        crate::handlers::email_verification::VerifyEmailQuery,
        crate::services::auth::VerifyEmailResult,
        crate::handlers::email_verification::ResendVerificationRequest,
        crate::services::auth::ResendVerificationResult,
        
        // Wallet Authentication types
        crate::handlers::wallet_auth::WalletRegistrationRequest,
        crate::handlers::wallet_auth::WalletRegistrationResponse,
        crate::handlers::wallet_auth::DevWalletInfo,
        crate::handlers::wallet_auth::WalletLoginResponse,
        crate::handlers::wallet_auth::UserWalletInfo,
        
        // Trading handler types
        crate::handlers::trading::OrderQuery,
        crate::handlers::trading::CreateOrderResponse,
        crate::handlers::trading::TradingStats,
        crate::handlers::trading::BlockchainMarketData,
        crate::handlers::trading::CreateBlockchainOrderRequest,
        crate::handlers::trading::CreateBlockchainOrderResponse,
        crate::handlers::trading::MatchOrdersResponse,
        
        // Blockchain handler types
        crate::handlers::blockchain::TransactionQuery,
        crate::handlers::blockchain::TransactionResponse,
        crate::handlers::blockchain::AccountInfo,
        crate::handlers::blockchain::NetworkStatus,
        crate::handlers::blockchain::ProgramInteractionRequest,
        
        // Meter handler types
        crate::handlers::meter::SubmitReadingRequest,
        crate::handlers::meter::MeterReadingResponse,
        crate::handlers::meter::GetReadingsQuery,
        crate::handlers::meter::MintFromReadingRequest,
        crate::handlers::meter::MintResponse,
        crate::handlers::meter::UserStatsResponse,
        
        // Token handler types
        crate::handlers::token::TokenBalanceResponse,
        crate::handlers::token::TokenInfoResponse,
        crate::handlers::token::MintTokensRequest,
        crate::handlers::token::MintTokensResponse,
        crate::handlers::token::MintFromReadingRequest,
        crate::handlers::token::MintFromReadingResponse,
        
        // Registry handler types
        crate::handlers::registry::UserType,
        crate::handlers::registry::UserStatus,
        crate::handlers::registry::MeterType,
        crate::handlers::registry::MeterStatus,
        crate::handlers::registry::BlockchainUserAccount,
        crate::handlers::registry::BlockchainMeterAccount,
        crate::handlers::registry::UpdateUserRoleRequest,
        
        // Governance handler types
        crate::handlers::governance::GovernanceStatusResponse,
        crate::handlers::governance::EmergencyPauseRequest,
        crate::handlers::governance::EmergencyActionResponse,
        
        // Oracle handler types
        crate::handlers::oracle::SubmitPriceRequest,
        crate::handlers::oracle::PriceSubmissionResponse,
        crate::handlers::oracle::CurrentPriceData,
        crate::handlers::oracle::OracleDataResponse,
        
        // ERC handler types
        crate::handlers::erc::IssueErcRequest,
        crate::handlers::erc::ErcCertificateResponse,
        crate::handlers::erc::GetCertificatesQuery,
        crate::handlers::erc::CertificateStatsResponse,
        
        // Blockchain test handler types
        crate::handlers::blockchain_test::CreateTestTransactionRequest,
        crate::handlers::blockchain_test::TestTransactionResponse,
        crate::handlers::blockchain_test::TestStatisticsResponse,
        
        // WebSocket handler types
        crate::handlers::websocket::WsMessage,
        crate::handlers::websocket::OrderBookEntry,
        crate::handlers::websocket::WsParams,
        crate::handlers::websocket::OrderBookData,
        
        // Admin handler types
        crate::handlers::admin::MarketHealth,
        crate::handlers::admin::OrderBookHealth,
        crate::handlers::admin::MatchingStatistics,
        crate::handlers::admin::SettlementStatistics,
        crate::handlers::admin::TradingAnalytics,
        crate::handlers::admin::PriceStatistics,
        crate::handlers::admin::TraderStats,
        crate::handlers::admin::HourlyVolume,
        crate::handlers::admin::MarketControlRequest,
        crate::handlers::admin::MarketAction,
        crate::handlers::admin::MarketControlResponse,
        
        // Database schema types
        crate::database::schema::types::UserRole,
        crate::database::schema::types::OrderType,
        crate::database::schema::types::OrderSide,
        crate::database::schema::types::OrderStatus,
        
        // User models
        crate::models::user::User,
        crate::models::user::CreateUserRequest,
        crate::models::user::UserProfile,
        crate::models::user::UserBalances,
        
        // Blockchain models
        crate::models::blockchain::TransactionSubmission,
        crate::models::blockchain::TransactionStatus,
        crate::models::blockchain::ProgramInteraction,
        
        // Trading models
        crate::models::trading::TradingOrder,
        crate::models::trading::CreateOrderRequest,
        crate::models::trading::MarketData,
        crate::models::trading::OrderBook,
        crate::models::trading::TradeExecution,
        
        // Energy meter models
        crate::models::energy::EnergyReading,
        crate::models::energy::EnergyReadingSubmission,
        crate::models::energy::EnergyMetadata,
    )),
    modifiers(&SecurityAddon),
    tags(
        (name = "health", description = "Health check endpoints"),
        (name = "auth", description = "Authentication and authorization"),
        (name = "users", description = "User management"),
        (name = "blockchain", description = "Blockchain interaction"),
        (name = "blockchain-test", description = "Blockchain testing utilities"),
        (name = "trading", description = "Energy trading operations"),
        (name = "meters", description = "Smart meter readings"),
        (name = "erc", description = "Energy Renewable Certificates"),
        (name = "tokens", description = "Token operations"),
        (name = "oracle", description = "Oracle price feeds"),
        (name = "governance", description = "Governance operations"),
        (name = "websocket", description = "WebSocket real-time data streams"),
        (name = "Admin - Market", description = "Market health monitoring and control (admin only)"),
    )
)]
pub struct ApiDoc;

struct SecurityAddon;

impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.as_mut().unwrap();
        components.add_security_scheme(
            "bearer_auth",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .description(Some("Enter your JWT token"))
                    .build()
            )
        )
    }
}
