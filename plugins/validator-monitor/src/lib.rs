use chrono::{DateTime, Duration, Utc};
use nearx_plugin_core::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ValidatorStats {
    name: String,
    last_block_height: u64,
    last_block_time: DateTime<Utc>,
    blocks_produced: u64,
    blocks_expected: u64,
    uptime_percentage: f64,
    avg_block_time_ms: u64,
    missed_blocks: Vec<u64>,
    alerts: Vec<Alert>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Alert {
    timestamp: DateTime<Utc>,
    alert_type: AlertType,
    message: String,
    severity: Severity,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum Severity {
    Info,
    Warning,
    Critical,
}

pub struct ValidatorMonitorPlugin {
    host: Arc<dyn PluginHost>,
    validators: Arc<Mutex<HashMap<String, ValidatorStats>>>,
    config: PluginConfig,
}

impl ValidatorMonitorPlugin {
    pub fn new(host: Arc<dyn PluginHost>) -> Self {
        Self {
            host,
            validators: Arc::new(Mutex::new(HashMap::new())),
            config: PluginConfig {
                uptime_threshold: 95.0,
                max_block_time_ms: 5000,
                missed_blocks_alert_threshold: 3,
                check_interval_seconds: 60,
            },
        }
    }

    async fn check_validator_health(&self, validator: &str, stats: &ValidatorStats) -> Vec<Alert> {
        let mut alerts = Vec::new();
        let now = Utc::now();

        // Check uptime
        if stats.uptime_percentage < self.config.uptime_threshold {
            alerts.push(Alert {
                timestamp: now,
                alert_type: AlertType::LowUptime,
                message: format!(
                    "Validator {} uptime dropped to {:.1}%",
                    validator, stats.uptime_percentage
                ),
                severity: if stats.uptime_percentage < 90.0 {
                    Severity::Critical
                } else {
                    Severity::Warning
                },
            });
        }

        // Check for missed blocks
        if stats.missed_blocks.len() >= self.config.missed_blocks_alert_threshold {
            alerts.push(Alert {
                timestamp: now,
                alert_type: AlertType::MissedBlocks,
                message: format!(
                    "Validator {} missed {} blocks in recent period",
                    validator,
                    stats.missed_blocks.len()
                ),
                severity: Severity::Warning,
            });
        }

        // Check block production latency
        if stats.avg_block_time_ms > self.config.max_block_time_ms {
            alerts.push(Alert {
                timestamp: now,
                alert_type: AlertType::HighLatency,
                message: format!(
                    "Validator {} average block time is {}ms (threshold: {}ms)",
                    validator, stats.avg_block_time_ms, self.config.max_block_time_ms
                ),
                severity: Severity::Warning,
            });
        }

        // Check if validator is stalled
        let time_since_last_block = now - stats.last_block_time;
        if time_since_last_block > Duration::minutes(5) {
            alerts.push(Alert {
                timestamp: now,
                alert_type: AlertType::ValidatorStalled,
                message: format!(
                    "Validator {} hasn't produced a block in {} minutes",
                    validator,
                    time_since_last_block.num_minutes()
                ),
                severity: Severity::Critical,
            });
        }

        alerts
    }

    async fn update_stats(&self, validator: String, block_height: u64) {
        let mut validators = self.validators.lock().await;
        let now = Utc::now();

        let stats = validators
            .entry(validator.clone())
            .or_insert(ValidatorStats {
                name: validator.clone(),
                last_block_height: block_height,
                last_block_time: now,
                blocks_produced: 0,
                blocks_expected: 0,
                uptime_percentage: 100.0,
                avg_block_time_ms: 0,
                missed_blocks: Vec::new(),
                alerts: Vec::new(),
            });

        // Update block production stats
        stats.blocks_produced += 1;
        stats.last_block_height = block_height;
        stats.last_block_time = now;

        // Calculate uptime
        if stats.blocks_expected > 0 {
            stats.uptime_percentage =
                (stats.blocks_produced as f64 / stats.blocks_expected as f64) * 100.0;
        }
    }
}

#[async_trait]
impl Plugin for ValidatorMonitorPlugin {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            id: "validator-monitor".to_string(),
            name: "Validator Monitor".to_string(),
            version: "0.1.0".to_string(),
            author: "NEARx Team".to_string(),
            description: "Monitors NEAR validator performance and health".to_string(),
            capabilities: vec![
                Capability::ValidatorTracking,
                Capability::RealtimeUpdates,
                Capability::CustomQueries,
            ],
        }
    }

    async fn init(&mut self) -> Result<()> {
        self.host
            .log(LogLevel::Info, "Validator Monitor plugin initialized");
        Ok(())
    }

    async fn handle_message(&mut self, message: PluginMessage) -> Result<Option<PluginMessage>> {
        match message {
            PluginMessage::BlockProduced {
                height,
                validator,
                tx_count,
                timestamp,
            } => {
                self.update_stats(validator.clone(), height).await;

                // Check validator health
                let validators = self.validators.lock().await;
                if let Some(stats) = validators.get(&validator) {
                    let alerts = self.check_validator_health(&validator, stats).await;

                    // Send alerts
                    for alert in alerts {
                        let alert_msg = PluginMessage::ValidatorAlert {
                            validator: validator.clone(),
                            alert_type: alert.alert_type,
                            message: alert.message,
                        };
                        self.host.send_message(alert_msg).await?;
                    }
                }
            }

            PluginMessage::Query {
                id,
                query: QueryType::GetValidatorStats(validator),
            } => {
                let validators = self.validators.lock().await;
                let data = if let Some(stats) = validators.get(&validator) {
                    serde_json::to_value(stats)?
                } else {
                    serde_json::Value::Null
                };

                return Ok(Some(PluginMessage::Response {
                    id,
                    data,
                    success: true,
                    error: None,
                }));
            }

            _ => {}
        }
        Ok(None)
    }

    async fn cleanup(&mut self) -> Result<()> {
        self.host
            .log(LogLevel::Info, "Validator Monitor plugin shutting down");
        Ok(())
    }

    fn subscriptions(&self) -> Vec<SubscriptionTopic> {
        vec![SubscriptionTopic::AllBlocks]
    }

    async fn tick(&mut self) -> Result<()> {
        // Periodic health checks
        let validators = self.validators.lock().await.clone();
        for (validator, stats) in validators.iter() {
            let alerts = self.check_validator_health(validator, stats).await;
            for alert in alerts {
                self.host.log(
                    match alert.severity {
                        Severity::Info => LogLevel::Info,
                        Severity::Warning => LogLevel::Warn,
                        Severity::Critical => LogLevel::Error,
                    },
                    &alert.message,
                );
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PluginConfig {
    uptime_threshold: f64,
    max_block_time_ms: u64,
    missed_blocks_alert_threshold: usize,
    check_interval_seconds: u64,
}

pub struct ValidatorMonitorFactory {
    host: Arc<dyn PluginHost>,
}

impl ValidatorMonitorFactory {
    pub fn new(host: Arc<dyn PluginHost>) -> Self {
        Self { host }
    }
}

impl PluginFactory for ValidatorMonitorFactory {
    fn create(&self, host: Arc<dyn PluginHost>) -> Result<Box<dyn Plugin>> {
        Ok(Box::new(ValidatorMonitorPlugin::new(host)))
    }

    fn info(&self) -> PluginInfo {
        PluginInfo {
            id: "validator-monitor".to_string(),
            name: "Validator Monitor".to_string(),
            version: "0.1.0".to_string(),
            author: "NEARx Team".to_string(),
            description: "Monitors NEAR validator performance and health".to_string(),
            capabilities: vec![
                Capability::ValidatorTracking,
                Capability::RealtimeUpdates,
                Capability::CustomQueries,
            ],
        }
    }
}
