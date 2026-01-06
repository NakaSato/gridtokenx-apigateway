use anyhow::{anyhow, Result};
use lettre::{
    message::{header::ContentType, Mailbox},
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use tracing::{error, info};

use super::types::EmailTemplate;

/// Email sender service
#[derive(Clone)]
pub struct EmailService {
    smtp_host: String,
    smtp_port: u16,
    smtp_username: String,
    smtp_password: String,
    from_email: String,
    from_name: String,
    enabled: bool,
}

impl std::fmt::Debug for EmailService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmailService")
            .field("smtp_host", &self.smtp_host)
            .field("from_email", &self.from_email)
            .field("enabled", &self.enabled)
            .finish()
    }
}

impl EmailService {
    pub fn new() -> Self {
        let smtp_host = std::env::var("SMTP_HOST").unwrap_or_else(|_| "smtp.gmail.com".to_string());
        let smtp_port = std::env::var("SMTP_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(587);
        let smtp_username = std::env::var("SMTP_USERNAME").unwrap_or_default();
        let smtp_password = std::env::var("SMTP_PASSWORD").unwrap_or_default();
        let from_email = std::env::var("SMTP_FROM_EMAIL")
            .unwrap_or_else(|_| "noreply@gridtokenx.com".to_string());
        let from_name = std::env::var("SMTP_FROM_NAME")
            .unwrap_or_else(|_| "GridTokenX Platform".to_string());
        let enabled = std::env::var("SMTP_ENABLED")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(false);

        Self {
            smtp_host,
            smtp_port,
            smtp_username,
            smtp_password,
            from_email,
            from_name,
            enabled,
        }
    }

    /// Check if email service is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled && !self.smtp_username.is_empty()
    }

    /// Send an email using a template
    pub async fn send_email(
        &self,
        to_email: &str,
        to_name: &str,
        template: EmailTemplate,
    ) -> Result<()> {
        if !self.is_enabled() {
            info!("Email service disabled, skipping email to {}", to_email);
            return Ok(());
        }

        let subject = template.subject();
        let body = self.render_template(&template)?;

        self.send_raw(to_email, to_name, &subject, &body).await
    }

    /// Send a raw email
    async fn send_raw(
        &self,
        to_email: &str,
        to_name: &str,
        subject: &str,
        body: &str,
    ) -> Result<()> {
        let from_mailbox: Mailbox = format!("{} <{}>", self.from_name, self.from_email)
            .parse()
            .map_err(|e| anyhow!("Invalid from address: {}", e))?;

        let to_mailbox: Mailbox = format!("{} <{}>", to_name, to_email)
            .parse()
            .map_err(|e| anyhow!("Invalid to address: {}", e))?;

        let email = Message::builder()
            .from(from_mailbox)
            .to(to_mailbox)
            .subject(subject)
            .header(ContentType::TEXT_HTML)
            .body(body.to_string())
            .map_err(|e| anyhow!("Failed to build email: {}", e))?;

        let credentials = Credentials::new(
            self.smtp_username.clone(),
            self.smtp_password.clone(),
        );

        let mailer: AsyncSmtpTransport<Tokio1Executor> = 
            AsyncSmtpTransport::<Tokio1Executor>::relay(&self.smtp_host)?
                .credentials(credentials)
                .port(self.smtp_port)
                .build();

        match mailer.send(email).await {
            Ok(_) => {
                info!("📧 Email sent to {}: {}", to_email, subject);
                Ok(())
            }
            Err(e) => {
                error!("Failed to send email to {}: {}", to_email, e);
                Err(anyhow!("Failed to send email: {}", e))
            }
        }
    }

    /// Render email template to HTML
    fn render_template(&self, template: &EmailTemplate) -> Result<String> {
        let (title, content) = match template {
            EmailTemplate::TradeMatched(data) => (
                "Order Matched",
                format!(
                    r#"
                    <p>Great news! Your {} order has been matched.</p>
                    <table style="border-collapse: collapse; margin: 20px 0;">
                        <tr><td style="padding: 8px; border: 1px solid #ddd;"><strong>Energy Amount:</strong></td><td style="padding: 8px; border: 1px solid #ddd;">{} kWh</td></tr>
                        <tr><td style="padding: 8px; border: 1px solid #ddd;"><strong>Price:</strong></td><td style="padding: 8px; border: 1px solid #ddd;">{} GRIDX/kWh</td></tr>
                        <tr><td style="padding: 8px; border: 1px solid #ddd;"><strong>Total Value:</strong></td><td style="padding: 8px; border: 1px solid #ddd;">{} GRIDX</td></tr>
                    </table>
                    <p>Settlement will proceed automatically.</p>
                    "#,
                    data.side, data.energy_amount, data.price_per_kwh, data.total_value
                ),
            ),
            EmailTemplate::SettlementComplete(data) => (
                "Settlement Complete",
                format!(
                    r#"
                    <p>Your energy trade settlement has been completed successfully.</p>
                    <table style="border-collapse: collapse; margin: 20px 0;">
                        <tr><td style="padding: 8px; border: 1px solid #ddd;"><strong>Energy Amount:</strong></td><td style="padding: 8px; border: 1px solid #ddd;">{} kWh</td></tr>
                        <tr><td style="padding: 8px; border: 1px solid #ddd;"><strong>Total Value:</strong></td><td style="padding: 8px; border: 1px solid #ddd;">{} GRIDX</td></tr>
                        {}
                    </table>
                    "#,
                    data.energy_amount,
                    data.total_value,
                    data.tx_signature.as_ref().map(|tx| format!(
                        r#"<tr><td style="padding: 8px; border: 1px solid #ddd;"><strong>Transaction:</strong></td><td style="padding: 8px; border: 1px solid #ddd;"><a href="https://explorer.solana.com/tx/{}?cluster=devnet">{}</a></td></tr>"#,
                        tx, &tx[..16]
                    )).unwrap_or_default()
                ),
            ),
            EmailTemplate::RecIssued(data) => (
                "REC Certificate Issued",
                format!(
                    r#"
                    <p>Congratulations! A Renewable Energy Certificate has been issued for your energy sale.</p>
                    <table style="border-collapse: collapse; margin: 20px 0;">
                        <tr><td style="padding: 8px; border: 1px solid #ddd;"><strong>Certificate ID:</strong></td><td style="padding: 8px; border: 1px solid #ddd;">{}</td></tr>
                        <tr><td style="padding: 8px; border: 1px solid #ddd;"><strong>Energy Amount:</strong></td><td style="padding: 8px; border: 1px solid #ddd;">{} kWh</td></tr>
                        <tr><td style="padding: 8px; border: 1px solid #ddd;"><strong>Source:</strong></td><td style="padding: 8px; border: 1px solid #ddd;">{}</td></tr>
                    </table>
                    <p>You can view your certificates in your dashboard.</p>
                    "#,
                    data.certificate_id, data.kwh_amount, data.renewable_source
                ),
            ),
        };

        Ok(format!(
            r#"
            <!DOCTYPE html>
            <html>
            <head>
                <meta charset="utf-8">
                <style>
                    body {{ font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif; line-height: 1.6; color: #333; max-width: 600px; margin: 0 auto; padding: 20px; }}
                    .header {{ background: linear-gradient(135deg, #667eea 0%, #764ba2 100%); color: white; padding: 20px; border-radius: 8px 8px 0 0; }}
                    .content {{ background: #f9f9f9; padding: 20px; border-radius: 0 0 8px 8px; }}
                    .footer {{ text-align: center; color: #666; font-size: 12px; margin-top: 20px; }}
                </style>
            </head>
            <body>
                <div class="header">
                    <h1 style="margin: 0;">🔋 GridTokenX</h1>
                    <h2 style="margin: 10px 0 0;">{}</h2>
                </div>
                <div class="content">
                    {}
                </div>
                <div class="footer">
                    <p>This is an automated message from GridTokenX Platform.</p>
                    <p>© 2026 GridTokenX. All rights reserved.</p>
                </div>
            </body>
            </html>
            "#,
            title, content
        ))
    }
}
