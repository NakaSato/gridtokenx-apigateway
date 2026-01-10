use chrono::{DateTime, Utc};
use serde::Serialize;
use crate::handlers::meter::types::ReadingData;

/// Alert severity levels
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

/// Meter alert for abnormal readings
#[derive(Debug, Clone, Serialize)]
pub struct MeterAlert {
    pub meter_id: String,
    pub alert_type: String,
    pub value: f64,
    pub threshold: f64,
    pub severity: AlertSeverity,
    pub message: String,
    pub timestamp: DateTime<Utc>,
}

/// Check for abnormal readings and generate alerts
pub fn check_alerts<T: ReadingData>(
    meter_id: &str,
    data: &T,
) -> Vec<MeterAlert> {
    let mut alerts = Vec::new();
    let now = Utc::now();

    // Voltage alerts
    if let Some(voltage) = data.voltage() {
        if voltage < 200.0 {
            alerts.push(MeterAlert {
                meter_id: meter_id.to_string(),
                alert_type: "low_voltage".to_string(),
                value: voltage,
                threshold: 200.0,
                severity: AlertSeverity::Critical,
                message: format!("Low voltage detected: {:.1}V (threshold: 200V)", voltage),
                timestamp: now,
            });
        } else if voltage > 260.0 {
            alerts.push(MeterAlert {
                meter_id: meter_id.to_string(),
                alert_type: "high_voltage".to_string(),
                value: voltage,
                threshold: 260.0,
                severity: AlertSeverity::Critical,
                message: format!("High voltage detected: {:.1}V (threshold: 260V)", voltage),
                timestamp: now,
            });
        }
    }

    // Frequency alerts
    if let Some(frequency) = data.frequency() {
        if frequency < 49.5 || frequency > 50.5 {
            alerts.push(MeterAlert {
                meter_id: meter_id.to_string(),
                alert_type: "frequency_deviation".to_string(),
                value: frequency,
                threshold: if frequency < 49.5 { 49.5 } else { 50.5 },
                severity: AlertSeverity::Warning,
                message: format!("Frequency deviation: {:.2}Hz (normal: 49.5-50.5Hz)", frequency),
                timestamp: now,
            });
        }
    }

    // Battery alerts
    if let Some(battery) = data.battery_level() {
        if battery < 20.0 {
            alerts.push(MeterAlert {
                meter_id: meter_id.to_string(),
                alert_type: "low_battery".to_string(),
                value: battery,
                threshold: 20.0,
                severity: if battery < 10.0 { AlertSeverity::Critical } else { AlertSeverity::Warning },
                message: format!("Low battery: {:.0}%", battery),
                timestamp: now,
            });
        }
    }

    // Power factor alerts
    if let Some(pf) = data.power_factor() {
        if pf < 0.8 {
            alerts.push(MeterAlert {
                meter_id: meter_id.to_string(),
                alert_type: "poor_power_factor".to_string(),
                value: pf,
                threshold: 0.8,
                severity: AlertSeverity::Warning,
                message: format!("Poor power factor: {:.2} (threshold: 0.8)", pf),
                timestamp: now,
            });
        }
    }

    // THD alerts
    if let Some(thd_v) = data.thd_voltage() {
        if thd_v > 5.0 {
            alerts.push(MeterAlert {
                meter_id: meter_id.to_string(),
                alert_type: "high_thd_voltage".to_string(),
                value: thd_v,
                threshold: 5.0,
                severity: AlertSeverity::Warning,
                message: format!("High THD voltage: {:.1}% (threshold: 5%)", thd_v),
                timestamp: now,
            });
        }
    }

    if let Some(thd_i) = data.thd_current() {
        if thd_i > 8.0 {
            alerts.push(MeterAlert {
                meter_id: meter_id.to_string(),
                alert_type: "high_thd_current".to_string(),
                value: thd_i,
                threshold: 8.0,
                severity: AlertSeverity::Warning,
                message: format!("High THD current: {:.1}% (threshold: 8%)", thd_i),
                timestamp: now,
            });
        }
    }

    alerts
}

/// Calculate health score (0-100) based on electrical parameters
pub fn calculate_health_score<T: ReadingData>(data: &T) -> f64 {
    let mut total_weight = 0.0;
    let mut weighted_score = 0.0;

    // Voltage score (30% weight) - optimal range 220-240V
    if let Some(voltage) = data.voltage() {
        let voltage_score = if voltage >= 220.0 && voltage <= 240.0 {
            100.0
        } else if voltage >= 200.0 && voltage <= 260.0 {
            let deviation = if voltage < 220.0 { 220.0 - voltage } else { voltage - 240.0 };
            100.0 - (deviation * 5.0).min(50.0)
        } else {
            25.0 // Very poor
        };
        weighted_score += voltage_score * 0.3;
        total_weight += 0.3;
    }

    // Power factor score (30% weight)
    if let Some(pf) = data.power_factor() {
        let pf_score = (pf * 100.0).min(100.0);
        weighted_score += pf_score * 0.3;
        total_weight += 0.3;
    }

    // THD score (20% weight) - lower is better
    let thd_total = data.thd_voltage().unwrap_or(0.0) + data.thd_current().unwrap_or(0.0);
    if data.thd_voltage().is_some() || data.thd_current().is_some() {
        let thd_score = (100.0 - thd_total * 5.0).max(0.0);
        weighted_score += thd_score * 0.2;
        total_weight += 0.2;
    }

    // Battery score (20% weight)
    if let Some(battery) = data.battery_level() {
        weighted_score += battery * 0.2;
        total_weight += 0.2;
    }

    // Normalize if not all components available
    if total_weight > 0.0 {
        (weighted_score / total_weight).min(100.0).max(0.0)
    } else {
        50.0 // Default neutral score
    }
}
