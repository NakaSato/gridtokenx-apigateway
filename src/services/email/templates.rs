/// Email templates for the GridTokenX Platform
/// Provides HTML email templates for verification, welcome, and other notifications

pub struct EmailTemplates;

impl EmailTemplates {
    /// HTML email template for email verification
    pub fn verification_email(username: &str, verification_url: &str) -> String {
        format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Verify Your Email - GridTokenX</title>
</head>
<body style="margin: 0; padding: 0; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; background-color: #f5f5f5;">
  <table role="presentation" style="width: 100%; border-collapse: collapse; background-color: #f5f5f5;">
    <tr>
      <td align="center" style="padding: 40px 0;">
        <table role="presentation" style="width: 600px; max-width: 100%; border-collapse: collapse; background-color: #ffffff; box-shadow: 0 4px 6px rgba(0, 0, 0, 0.1);">
          
          <!-- Body -->
          <tr>
            <td style="padding: 40px 30px; background-color: #ffffff;">
              <h2 style="color: #1f2937; margin: 0 0 20px 0; font-size: 24px; font-weight: 600;">Welcome, {}!</h2>
              
              <p style="color: #4b5563; line-height: 1.6; margin: 0 0 20px 0; font-size: 16px;">
                Thank you for registering with <strong>GridTokenX</strong> Platform. We're excited to have you join our 
                peer-to-peer energy trading network!
              </p>
              
              <p style="color: #4b5563; line-height: 1.6; margin: 0 0 30px 0; font-size: 16px;">
                To complete your registration and start trading energy tokens, please verify your email address 
                by clicking the button below:
              </p>
              
              <!-- Button -->
              <table role="presentation" style="width: 100%; border-collapse: collapse;">
                <tr>
                  <td align="center" style="padding: 0 0 30px 0;">
                    <a href="{}" 
                      style="display: inline-block; background: linear-gradient(135deg, #10b981 0%, #059669 100%); 
                          color: #ffffff; padding: 16px 40px; text-decoration: none; 
                          font-weight: 600; font-size: 16px; box-shadow: 0 4px 6px rgba(16, 185, 129, 0.4);">
                      Verify Email Address
                    </a>
                  </td>
                </tr>
              </table>
              
              <!-- Fallback Link -->
              <p style="color: #6b7280; font-size: 14px; line-height: 1.6; margin: 0 0 10px 0;">
                If the button doesn't work, copy and paste this link into your browser:
              </p>
              <p style="background-color: #f3f4f6; padding: 12px; 
                    font-size: 13px; color: #10b981; margin: 0 0 30px 0;">
                <a href="{}" style="color: #10b981; text-decoration: none;">{}</a>
              </p>
              <p style="color: #6b7280; margin: 0; font-size: 14px; line-height: 1.5; text-underline-offset: inherit;">
                This verification link will expire in 24 hours for security purposes.
              </p>
              <p style="color: #6b7280; font-size: 14px; line-height: 1.6; margin: 0;">
                If you didn't create an account with GridTokenX, you can safely ignore this email.
              </p>
            </td>
          </tr>
          
          <!-- Footer -->
          <tr>
            <td style="background-color: #f9fafb; padding: 10px; text-align: center; border-top: 1px solid #e5e7eb;">
              <p style="color: #9ca3af; margin: 0 0 10px 0; font-size: 13px;">
                © 2025 GridTokenX Platform. All rights reserved.
              </p>
              <p style="color: #9ca3af; margin: 0; font-size: 12px;">
                This is an automated email. Please do not reply to this message.
              </p>
            </td>
          </tr>
        </table>
      </td>
    </tr>
  </table>
</body>
</html>"#,
            username, verification_url, verification_url, verification_url
        )
    }

    /// HTML email template for welcome message after verification
    pub fn welcome_email(username: &str, dashboard_url: &str) -> String {
        format!(
            r#"<!DOCTYPE html>
            <html lang="en">
            <head>
                <meta charset="UTF-8">
                <meta name="viewport" content="width=device-width, initial-scale=1.0">
                <title>Welcome to GridTokenX!</title>
            </head>
            <body style="margin: 0; padding: 0; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; background-color: #f5f5f5;">
                <table role="presentation" style="width: 100%; border-collapse: collapse; background-color: #f5f5f5;">
                    <tr>
                        <td align="center" style="padding: 40px 0;">
                            <table role="presentation" style="width: 600px; max-width: 100%; border-collapse: collapse; background-color: #ffffff; box-shadow: 0 4px 6px rgba(0, 0, 0, 0.1);">
                                <!-- Header -->
                                <tr>
                                    <td style="background: linear-gradient(135deg, #667eea 0%, #764ba2 100%); padding: 40px 30px; text-align: center; border-radius: 10px 10px 0 0;">
                                        <h1 style="color: #ffffff; margin: 0; font-size: 32px; font-weight: 600;">Welcome to GridTokenX! 🎉</h1>
                                        <p style="color: #e0e7ff; margin: 10px 0 0 0; font-size: 14px;">Your email has been verified</p>
                                    </td>
                                </tr>
                                
                                <!-- Body -->
                                <tr>
                                    <td style="padding: 40px 30px; background-color: #ffffff;">
                                        <h2 style="color: #1f2937; margin: 0 0 20px 0; font-size: 24px; font-weight: 600;">Hello, {}!</h2>
                                        
                                        <p style="color: #4b5563; line-height: 1.6; margin: 0 0 20px 0; font-size: 16px;">
                                            Congratulations! Your email has been successfully verified. You now have full access to all features 
                                            of the GridTokenX Platform.
                                        </p>
                                        
                                        <h3 style="color: #1f2937; margin: 30px 0 15px 0; font-size: 20px; font-weight: 600;">What's Next?</h3>
                                        
                                        <ul style="color: #4b5563; line-height: 1.8; margin: 0 0 30px 0; padding-left: 20px; font-size: 15px;">
                                            <li style="margin-bottom: 10px;">
                                                <strong>Connect Your Wallet:</strong> Link your Solana wallet for blockchain transactions
                                            </li>
                                            <li style="margin-bottom: 10px;">
                                                <strong>View Dashboard:</strong> Monitor your energy consumption and production in real-time
                                            </li>
                                            <li style="margin-bottom: 10px;">
                                                <strong>Start Trading:</strong> Buy and sell energy tokens with other users on the platform
                                            </li>
                                            <li style="margin-bottom: 10px;">
                                                <strong>Track Prices:</strong> View live energy market prices and trends
                                            </li>
                                            <li style="margin-bottom: 10px;">
                                                <strong>Manage Meters:</strong> Connect and manage your smart meters
                                            </li>
                                        </ul>
                                        
                                        <!-- Button -->
                                        <table role="presentation" style="width: 100%; border-collapse: collapse;">
                                            <tr>
                                                <td align="center" style="padding: 0 0 30px 0;">
                                                    <a href="{}" 
                                                      style="display: inline-block; background: linear-gradient(135deg, #667eea 0%, #764ba2 100%); 
                                                              color: #ffffff; padding: 16px 40px; text-decoration: none; border-radius: 8px; 
                                                              font-weight: 600; font-size: 16px; box-shadow: 0 4px 6px rgba(102, 126, 234, 0.4);">
                                                        Go to Dashboard
                                                    </a>
                                                </td>
                                            </tr>
                                        </table>
                                        
                                        <!-- Help Section -->
                                        <div style="background-color: #eff6ff; border-left: 4px solid #3b82f6; padding: 16px; border-radius: 6px; margin: 0 0 20px 0;">
                                            <p style="color: #1e40af; margin: 0 0 10px 0; font-size: 14px; font-weight: 600;">
                                                Need Help?
                                            </p>
                                            <p style="color: #1e40af; margin: 0; font-size: 14px; line-height: 1.5;">
                                                If you have any questions or need assistance, feel free to contact our support team 
                                                or visit our help center.
                                            </p>
                                        </div>
                                        
                                        <p style="color: #4b5563; line-height: 1.6; margin: 0; font-size: 16px;">
                                            Thank you for joining GridTokenX. Together, we're building a sustainable energy future!
                                        </p>
                                    </td>
                                </tr>
                                
                                <!-- Footer -->
                                <tr>
                                    <td style="background-color: #f9fafb; padding: 30px; text-align: center; border-radius: 0 0 10px 10px; border-top: 1px solid #e5e7eb;">
                                        <p style="color: #9ca3af; margin: 0 0 10px 0; font-size: 13px;">
                                            © 2025 GridTokenX Platform. All rights reserved.
                                        </p>
                                        <p style="color: #9ca3af; margin: 0; font-size: 12px;">
                                            This is an automated email. Please do not reply to this message.
                                        </p>
                                    </td>
                                </tr>
                            </table>
                        </td>
                    </tr>
                </table>
            </body>
            </html>"#,
            username, dashboard_url
        )
    }

    /// Plain text email template for email verification
    pub fn verification_email_text(username: &str, verification_url: &str) -> String {
        format!(
            r#"Welcome to GridTokenX Platform!

            Hello {},

            Thank you for registering with GridTokenX Platform. We're excited to have you join our peer-to-peer energy trading network!

            To complete your registration and start trading energy tokens, please verify your email address by visiting this link:

            {}

            IMPORTANT: This verification link will expire in 24 hours for security purposes.

            If you didn't create an account with GridTokenX, you can safely ignore this email.

            ---
            © 2025 GridTokenX Platform. All rights reserved.
            This is an automated email. Please do not reply to this message.
            "#,
            username, verification_url
        )
    }

    /// Plain text email template for welcome message after verification
    pub fn welcome_email_text(username: &str, dashboard_url: &str) -> String {
        format!(
            r#"Welcome to GridTokenX!

            Hello {},

            Congratulations! Your email has been successfully verified. You now have full access to all features of the GridTokenX Platform.

            What's Next?

            * Connect Your Wallet: Link your Solana wallet for blockchain transactions
            * View Dashboard: Monitor your energy consumption and production in real-time
            * Start Trading: Buy and sell energy tokens with other users on the platform
            * Track Prices: View live energy market prices and trends
            * Manage Meters: Connect and manage your smart meters

            Get started by visiting your dashboard:
            {}

            Need Help?
            If you have any questions or need assistance, feel free to contact our support team or visit our help center.

            Thank you for joining GridTokenX. Together, we're building a sustainable energy future!

            ---
            © 2025 GridTokenX Platform. All rights reserved.
            This is an automated email. Please do not reply to this message.
            "#,
            username, dashboard_url
        )
    }

    /// HTML email template for password reset
    pub fn password_reset_email(username: &str, reset_url: &str) -> String {
        format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Reset Your Password - GridTokenX</title>
</head>
<body style="margin: 0; padding: 0; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; background-color: #f5f5f5;">
  <table role="presentation" style="width: 100%; border-collapse: collapse; background-color: #f5f5f5;">
    <tr>
      <td align="center" style="padding: 40px 0;">
        <table role="presentation" style="width: 600px; max-width: 100%; border-collapse: collapse; background-color: #ffffff; box-shadow: 0 4px 6px rgba(0, 0, 0, 0.1);">
          
          <!-- Body -->
          <tr>
            <td style="padding: 40px 30px; background-color: #ffffff;">
              <h2 style="color: #1f2937; margin: 0 0 20px 0; font-size: 24px; font-weight: 600;">Password Reset Request</h2>
              
              <p style="color: #4b5563; line-height: 1.6; margin: 0 0 20px 0; font-size: 16px;">
                Hello <strong>{}</strong>,
              </p>
              
              <p style="color: #4b5563; line-height: 1.6; margin: 0 0 20px 0; font-size: 16px;">
                We received a request to reset the password for your <strong>GridTokenX</strong> account. 
                Click the button below to create a new password:
              </p>
              
              <!-- Button -->
              <table role="presentation" style="width: 100%; border-collapse: collapse;">
                <tr>
                  <td align="center" style="padding: 0 0 30px 0;">
                    <a href="{}" 
                      style="display: inline-block; background: linear-gradient(135deg, #ef4444 0%, #dc2626 100%); 
                          color: #ffffff; padding: 16px 40px; text-decoration: none; 
                          font-weight: 600; font-size: 16px; box-shadow: 0 4px 6px rgba(239, 68, 68, 0.4);">
                      Reset Password
                    </a>
                  </td>
                </tr>
              </table>
              
              <!-- Fallback Link -->
              <p style="color: #6b7280; font-size: 14px; line-height: 1.6; margin: 0 0 10px 0;">
                If the button doesn't work, copy and paste this link into your browser:
              </p>
              <p style="background-color: #f3f4f6; padding: 12px; 
                    font-size: 13px; color: #ef4444; margin: 0 0 30px 0;">
                <a href="{}" style="color: #ef4444; text-decoration: none;">{}</a>
              </p>
              
              <!-- Security Notice -->
              <div style="background-color: #fef3c7; border-left: 4px solid #f59e0b; padding: 16px; margin: 0 0 20px 0;">
                <p style="color: #92400e; margin: 0 0 10px 0; font-size: 14px; font-weight: 600;">
                  ⚠️ Security Notice
                </p>
                <p style="color: #92400e; margin: 0; font-size: 14px; line-height: 1.5;">
                  This link will expire in 1 hour. If you didn't request a password reset, 
                  please ignore this email or contact support if you have concerns.
                </p>
              </div>
              
              <p style="color: #6b7280; font-size: 14px; line-height: 1.6; margin: 0;">
                For security, this request was received from your account.
              </p>
            </td>
          </tr>
          
          <!-- Footer -->
          <tr>
            <td style="background-color: #f9fafb; padding: 10px; text-align: center; border-top: 1px solid #e5e7eb;">
              <p style="color: #9ca3af; margin: 0 0 10px 0; font-size: 13px;">
                © 2025 GridTokenX Platform. All rights reserved.
              </p>
              <p style="color: #9ca3af; margin: 0; font-size: 12px;">
                This is an automated email. Please do not reply to this message.
              </p>
            </td>
          </tr>
        </table>
      </td>
    </tr>
  </table>
</body>
</html>"#,
            username, reset_url, reset_url, reset_url
        )
    }

    /// Plain text email template for password reset
    pub fn password_reset_email_text(username: &str, reset_url: &str) -> String {
        format!(
            r#"Password Reset Request - GridTokenX

Hello {},

We received a request to reset the password for your GridTokenX account.

To reset your password, please visit this link:

{}

IMPORTANT: This link will expire in 1 hour for security purposes.

If you didn't request a password reset, please ignore this email or contact support if you have concerns.

---
© 2025 GridTokenX Platform. All rights reserved.
This is an automated email. Please do not reply to this message.
"#,
            username, reset_url
        )
    }

    /// HTML email template for generic notifications
    pub fn notification_email(username: &str, title: &str, message: &str) -> String {
        format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>{} - GridTokenX</title>
</head>
<body style="margin: 0; padding: 0; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; background-color: #f5f5f5;">
  <table role="presentation" style="width: 100%; border-collapse: collapse; background-color: #f5f5f5;">
    <tr>
      <td align="center" style="padding: 40px 0;">
        <table role="presentation" style="width: 600px; max-width: 100%; border-collapse: collapse; background-color: #ffffff; box-shadow: 0 4px 6px rgba(0, 0, 0, 0.1);">
          <!-- Header -->
          <tr>
            <td style="background: linear-gradient(135deg, #10b981 0%, #059669 100%); padding: 30px; text-align: center; border-radius: 8px 8px 0 0;">
              <h1 style="color: #ffffff; margin: 0; font-size: 24px; font-weight: 600;">GridTokenX</h1>
            </td>
          </tr>
          
          <!-- Body -->
          <tr>
            <td style="padding: 40px 30px; background-color: #ffffff;">
              <h2 style="color: #1f2937; margin: 0 0 20px 0; font-size: 20px; font-weight: 600;">{}</h2>
              
              <p style="color: #4b5563; line-height: 1.6; margin: 0 0 20px 0; font-size: 16px;">
                Hello <strong>{}</strong>,
              </p>
              
              <p style="color: #4b5563; line-height: 1.6; margin: 0 0 30px 0; font-size: 16px;">
                {}
              </p>
              
              <table role="presentation" style="width: 100%; border-collapse: collapse;">
                <tr>
                  <td align="center" style="padding: 0 0 30px 0;">
                    <a href="https://gridtokenx.com/dashboard" 
                      style="display: inline-block; background-color: #10b981; 
                          color: #ffffff; padding: 12px 30px; text-decoration: none; 
                          border-radius: 5px; font-weight: 600; font-size: 16px;">
                      View Dashboard
                    </a>
                  </td>
                </tr>
              </table>
              
              <p style="color: #6b7280; margin: 0; font-size: 14px; line-height: 1.5;">
                You are receiving this email because you enabled specific notifications in your profile settings.
              </p>
            </td>
          </tr>
          
          <!-- Footer -->
          <tr>
            <td style="background-color: #f9fafb; padding: 20px; text-align: center; border-top: 1px solid #e5e7eb;">
              <p style="color: #9ca3af; margin: 0; font-size: 12px;">
                © 2026 GridTokenX Platform. All rights reserved.
              </p>
            </td>
          </tr>
        </table>
      </td>
    </tr>
  </table>
</body>
</html>"#,
            title, title, username, message
        )
    }

    /// Plain text email template for generic notifications
    pub fn notification_email_text(username: &str, title: &str, message: &str) -> String {
        format!(
            r#"GridTokenX Notification: {}

Hello {},

{}

View your dashboard: https://gridtokenx.com/dashboard

---
You are receiving this email because you enabled specific notifications in your profile settings.
© 2026 GridTokenX Platform. All rights reserved.
"#,
            title, username, message
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verification_email_contains_username() {
        let email =
            EmailTemplates::verification_email("testuser", "http://example.com/verify?token=abc");
        assert!(email.contains("testuser"));
    }

    #[test]
    fn test_verification_email_contains_url() {
        let url = "http://example.com/verify?token=abc123";
        let email = EmailTemplates::verification_email("testuser", url);
        assert!(email.contains(url));
    }

    #[test]
    fn test_welcome_email_contains_username() {
        let email = EmailTemplates::welcome_email("testuser", "http://example.com/dashboard");
        assert!(email.contains("testuser"));
    }

    #[test]
    fn test_text_emails_are_generated() {
        let verification_text =
            EmailTemplates::verification_email_text("testuser", "http://example.com");
        let welcome_text = EmailTemplates::welcome_email_text("testuser", "http://example.com");

        assert!(!verification_text.is_empty());
        assert!(!welcome_text.is_empty());
        assert!(verification_text.contains("testuser"));
        assert!(welcome_text.contains("testuser"));
    }
}
