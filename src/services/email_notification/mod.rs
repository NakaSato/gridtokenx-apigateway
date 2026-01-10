//! Email Notification Service
//! 
//! Sends email notifications for trading events

use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};
use tracing::{info, error};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailConfig {
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_username: String,
    pub smtp_password: String,
    pub from_email: String,
    pub from_name: String,
}

impl Default for EmailConfig {
    fn default() -> Self {
        Self {
            smtp_host: std::env::var("SMTP_HOST").unwrap_or_else(|_| "smtp.gmail.com".to_string()),
            smtp_port: std::env::var("SMTP_PORT")
                .unwrap_or_else(|_| "587".to_string())
                .parse()
                .unwrap_or(587),
            smtp_username: std::env::var("SMTP_USERNAME").unwrap_or_default(),
            smtp_password: std::env::var("SMTP_PASSWORD").unwrap_or_default(),
            from_email: std::env::var("FROM_EMAIL").unwrap_or_else(|_| "noreply@gridtokenx.com".to_string()),
            from_name: std::env::var("FROM_NAME").unwrap_or_else(|_| "GridTokenX".to_string()),
        }
    }
}

pub struct EmailService {
    config: EmailConfig,
    mailer: Option<SmtpTransport>,
}

impl EmailService {
    pub fn new(config: EmailConfig) -> Self {
        let mailer = if !config.smtp_username.is_empty() {
            let creds = Credentials::new(
                config.smtp_username.clone(),
                config.smtp_password.clone(),
            );

            SmtpTransport::relay(&config.smtp_host)
                .ok()
                .map(|builder| builder.port(config.smtp_port).credentials(creds).build())
                .or_else(|| {
                    SmtpTransport::relay(&config.smtp_host)
                        .map(|b| b.build())
                        .ok()
                })
        } else {
            None
        };

        Self { config, mailer }
    }

    /// Send trade confirmation email
    pub async fn send_trade_confirmation(
        &self,
        to_email: &str,
        trade_type: &str,
        amount: f64,
        price: f64,
        total: f64,
        tx_signature: Option<&str>,
    ) -> Result<(), String> {
        let subject = format!("GridTokenX: Your {} Order Has Been Filled", trade_type);
        
        let explorer_link = tx_signature
            .map(|sig| format!("<p><a href=\"https://solscan.io/tx/{}?cluster=devnet\">View on Solscan</a></p>", sig))
            .unwrap_or_default();

        let body = format!(
            r#"
            <html>
            <body style="font-family: Arial, sans-serif; max-width: 600px; margin: 0 auto;">
                <div style="background: linear-gradient(135deg, #667eea 0%, #764ba2 100%); padding: 20px; text-align: center;">
                    <h1 style="color: white; margin: 0;">⚡ GridTokenX</h1>
                </div>
                <div style="padding: 20px; background: #f9fafb;">
                    <h2 style="color: #374151;">Trade Confirmed!</h2>
                    <p>Your {trade_type} order has been successfully filled.</p>
                    
                    <div style="background: white; padding: 15px; border-radius: 8px; margin: 15px 0;">
                        <table style="width: 100%; border-collapse: collapse;">
                            <tr>
                                <td style="padding: 8px 0; color: #6b7280;">Type</td>
                                <td style="padding: 8px 0; text-align: right; font-weight: bold;">{trade_type}</td>
                            </tr>
                            <tr>
                                <td style="padding: 8px 0; color: #6b7280;">Amount</td>
                                <td style="padding: 8px 0; text-align: right; font-weight: bold;">{amount:.2} kWh</td>
                            </tr>
                            <tr>
                                <td style="padding: 8px 0; color: #6b7280;">Price</td>
                                <td style="padding: 8px 0; text-align: right; font-weight: bold;">${price:.4}/kWh</td>
                            </tr>
                            <tr style="border-top: 1px solid #e5e7eb;">
                                <td style="padding: 8px 0; color: #6b7280;">Total</td>
                                <td style="padding: 8px 0; text-align: right; font-weight: bold; color: #10b981;">${total:.2}</td>
                            </tr>
                        </table>
                    </div>
                    
                    {explorer_link}
                    
                    <p style="color: #6b7280; font-size: 12px; margin-top: 20px;">
                        This is an automated message from GridTokenX. Please do not reply.
                    </p>
                </div>
            </body>
            </html>
            "#,
            trade_type = trade_type,
            amount = amount,
            price = price,
            total = total,
            explorer_link = explorer_link,
        );

        self.send_email(to_email, &subject, &body).await
    }

    /// Send order filled notification
    pub async fn send_order_filled_notification(
        &self,
        to_email: &str,
        order_id: &str,
        filled_amount: f64,
        fill_price: f64,
    ) -> Result<(), String> {
        let subject = "GridTokenX: Your Order Has Been Filled".to_string();
        
        let body = format!(
            r#"
            <html>
            <body style="font-family: Arial, sans-serif;">
                <h2>Order Filled</h2>
                <p>Your order <strong>{}</strong> has been filled.</p>
                <ul>
                    <li>Filled Amount: {:.2} kWh</li>
                    <li>Fill Price: ${:.4}/kWh</li>
                </ul>
                <p>Thank you for trading on GridTokenX!</p>
            </body>
            </html>
            "#,
            order_id, filled_amount, fill_price
        );

        self.send_email(to_email, &subject, &body).await
    }

    async fn send_email(&self, to: &str, subject: &str, body: &str) -> Result<(), String> {
        let Some(mailer) = &self.mailer else {
            info!("Email service not configured, skipping email to {}", to);
            return Ok(());
        };

        let email = Message::builder()
            .from(format!("{} <{}>", self.config.from_name, self.config.from_email).parse().map_err(|e| format!("Invalid from address: {}", e))?)
            .to(to.parse().map_err(|e| format!("Invalid to address: {}", e))?)
            .subject(subject)
            .header(lettre::message::header::ContentType::TEXT_HTML)
            .body(body.to_string())
            .map_err(|e| format!("Failed to build email: {}", e))?;

        match mailer.send(&email) {
            Ok(_) => {
                info!("Email sent successfully to {}", to);
                Ok(())
            }
            Err(e) => {
                error!("Failed to send email: {}", e);
                Err(format!("Failed to send email: {}", e))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_email_config_default() {
        let config = EmailConfig::default();
        assert_eq!(config.smtp_host, "smtp.gmail.com");
        assert_eq!(config.smtp_port, 587);
    }
}
