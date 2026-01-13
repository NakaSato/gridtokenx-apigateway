pub mod templates;

use anyhow::{Context, Result};
use lettre::{
    message::{header::ContentType, Mailbox, MultiPart, SinglePart},
    transport::smtp::authentication::Credentials,
    Message, SmtpTransport, Transport,
};
use tracing::{error, info};

use crate::config::EmailConfig;
use templates::EmailTemplates;

/// Email service for sending transactional emails
#[derive(Clone)]
pub struct EmailService {
    mailer: SmtpTransport,
    from_email: String,
    from_name: String,
    base_url: String,
    enabled: bool,
}

impl EmailService {
    /// Create a new email service from configuration
    pub fn new(config: &EmailConfig) -> Result<Self> {
        // Determine if we should use TLS based on port
        // Port 1025 is typically used for MailHog/local testing (no TLS)
        // Ports 587, 465 are typically used for production SMTP (with TLS)
        let use_tls = config.smtp_port != 1025;

        let mailer = if use_tls {
            // Production SMTP with TLS
            let creds =
                Credentials::new(config.smtp_username.clone(), config.smtp_password.clone());

            SmtpTransport::starttls_relay(&config.smtp_host)
                .context("Failed to create SMTP transport with TLS")?
                .port(config.smtp_port)
                .credentials(creds)
                .build()
        } else {
            // Development SMTP without TLS (e.g., MailHog)
            SmtpTransport::builder_dangerous(&config.smtp_host)
                .port(config.smtp_port)
                .build()
        };

        info!(
            "Email service initialized: {}:{} (TLS: {}, enabled: {})",
            config.smtp_host, config.smtp_port, use_tls, config.verification_enabled
        );

        Ok(Self {
            mailer,
            from_email: config.from_address.clone(),
            from_name: config.from_name.clone(),
            base_url: config.verification_base_url.clone(),
            enabled: config.verification_enabled,
        })
    }

    /// Send email verification message to user
    pub async fn send_verification_email(
        &self,
        to_email: &str,
        token: &str,
        username: &str,
    ) -> Result<()> {
        if !self.enabled {
            info!(
                "Email service disabled, skipping verification email to {}",
                to_email
            );
            return Ok(());
        }

        // Build verification URL
        let verification_url = format!("{}/verify-email?token={}", self.base_url, token);

        // Generate HTML and text content
        let html_body = EmailTemplates::verification_email(username, &verification_url);
        let text_body = EmailTemplates::verification_email_text(username, &verification_url);

        // Build and send email
        self.send_email(
            to_email,
            "Verify Your Email - GridTokenX Platform",
            &html_body,
            &text_body,
        )
        .await
        .context("Failed to send verification email")?;

        info!("Verification email sent to {}", to_email);
        Ok(())
    }

    /// Send welcome email after successful verification
    pub async fn send_welcome_email(&self, to_email: &str, username: &str) -> Result<()> {
        if !self.enabled {
            info!(
                "Email service disabled, skipping welcome email to {}",
                to_email
            );
            return Ok(());
        }

        // Build dashboard URL
        let dashboard_url = format!("{}/dashboard", self.base_url);

        // Generate HTML and text content
        let html_body = EmailTemplates::welcome_email(username, &dashboard_url);
        let text_body = EmailTemplates::welcome_email_text(username, &dashboard_url);

        // Build and send email
        self.send_email(
            to_email,
            "Welcome to GridTokenX! 🎉",
            &html_body,
            &text_body,
        )
        .await
        .context("Failed to send welcome email")?;

        info!("Welcome email sent to {}", to_email);
        Ok(())
    }

    /// Send password reset email
    pub async fn send_password_reset_email(
        &self,
        to_email: &str,
        token: &str,
        username: &str,
    ) -> Result<()> {
        if !self.enabled {
            info!(
                "Email service disabled, skipping password reset email to {}",
                to_email
            );
            return Ok(());
        }

        // Build reset URL
        let reset_url = format!("{}/reset-password?token={}", self.base_url, token);

        // Generate HTML and text content
        let html_body = EmailTemplates::password_reset_email(username, &reset_url);
        let text_body = EmailTemplates::password_reset_email_text(username, &reset_url);

        // Build and send email
        self.send_email(
            to_email,
            "Reset Your Password - GridTokenX Platform",
            &html_body,
            &text_body,
        )
        .await
        .context("Failed to send password reset email")?;

        info!("Password reset email sent to {}", to_email);
        Ok(())
    }

    /// Internal method to send email with HTML and text parts
    async fn send_email(
        &self,
        to_email: &str,
        subject: &str,
        html_body: &str,
        text_body: &str,
    ) -> Result<()> {
        // Parse mailboxes
        let from: Mailbox = format!("{} <{}>", self.from_name, self.from_email)
            .parse()
            .context("Failed to parse from address")?;

        let to: Mailbox = to_email
            .parse()
            .context("Failed to parse recipient address")?;

        // Build multipart email with HTML and plain text alternatives
        let email = Message::builder()
            .from(from)
            .to(to)
            .subject(subject)
            .multipart(
                MultiPart::alternative()
                    .singlepart(
                        SinglePart::builder()
                            .header(ContentType::TEXT_PLAIN)
                            .body(text_body.to_string()),
                    )
                    .singlepart(
                        SinglePart::builder()
                            .header(ContentType::TEXT_HTML)
                            .body(html_body.to_string()),
                    ),
            )
            .context("Failed to build email message")?;

        // Send email via SMTP
        match self.mailer.send(&email) {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("Failed to send email to {}: {}", to_email, e);
                Err(anyhow::anyhow!("Failed to send email: {}", e))
            }
        }
    }

    /// Check if email service is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Send a test email to verify configuration
    pub async fn send_test_email(&self, to_email: &str) -> Result<()> {
        if !self.enabled {
            return Err(anyhow::anyhow!("Email service is disabled"));
        }

        let html_body = r#"
<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>Email Test</title>
</head>
<body style="font-family: Arial, sans-serif; padding: 20px;">
    <h2>Email Configuration Test</h2>
    <p>This is a test email from GridTokenX Platform.</p>
    <p>If you received this email, your email configuration is working correctly!</p>
    <hr>
    <p style="color: #666; font-size: 12px;">GridTokenX Platform - Automated Test Email</p>
</body>
</html>
"#;

        let text_body = r#"
Email Configuration Test

This is a test email from GridTokenX Platform.
If you received this email, your email configuration is working correctly!

---
GridTokenX Platform - Automated Test Email
"#;

        self.send_email(
            to_email,
            "GridTokenX Email Configuration Test",
            html_body,
            text_body,
        )
        .await
        .context("Failed to send test email")?;

        info!("Test email sent to {}", to_email);
        Ok(())
    }

    /// Send a generic notification email
    pub async fn send_notification_email(
        &self,
        to_email: &str,
        username: &str,
        title: &str,
        message: &str,
    ) -> Result<()> {
        if !self.enabled {
            info!(
                "Email service disabled, skipping notification email to {}",
                to_email
            );
            return Ok(());
        }

        let html_body = templates::EmailTemplates::notification_email(username, title, message);
        let text_body = templates::EmailTemplates::notification_email_text(username, title, message);

        self.send_email(to_email, title, &html_body, &text_body).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_email_service_creation() {
        let config = EmailConfig {
            smtp_host: "smtp.example.com".to_string(),
            smtp_port: 587,
            smtp_username: "test@example.com".to_string(),
            smtp_password: "password".to_string(),
            from_name: "Test".to_string(),
            from_address: "test@example.com".to_string(),
            verification_expiry_hours: 24,
            verification_base_url: "http://localhost:3000".to_string(),
            verification_required: true,
            verification_enabled: false, // Disabled for tests
            auto_login_after_verification: false,
        };

        let service = EmailService::new(&config);
        assert!(service.is_ok());
    }

    #[test]
    fn test_email_service_disabled() {
        let config = EmailConfig {
            smtp_host: "smtp.example.com".to_string(),
            smtp_port: 587,
            smtp_username: "test@example.com".to_string(),
            smtp_password: "password".to_string(),
            from_name: "Test".to_string(),
            from_address: "test@example.com".to_string(),
            verification_expiry_hours: 24,
            verification_base_url: "http://localhost:3000".to_string(),
            verification_required: true,
            verification_enabled: false,
            auto_login_after_verification: false,
        };

        let service = EmailService::new(&config).unwrap();
        assert!(!service.is_enabled());
    }
}
