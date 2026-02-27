use base64::{engine::general_purpose, Engine as _};
use chrono::{DateTime, Utc};
use nearx_plugin_core::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TransactionPattern {
    pattern_type: PatternType,
    frequency: u64,
    examples: Vec<String>,
    last_seen: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
enum PatternType {
    HighValueTransfer,
    FrequentCaller,
    ContractDeployment,
    BatchTransaction,
    CrossContractCall,
    FailurePattern,
    GasOptimization,
    MEVActivity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DecodedAction {
    action_type: String,
    method_name: Option<String>,
    args_decoded: Option<serde_json::Value>,
    gas_attached: Option<u64>,
    deposit_attached: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TransactionAnalysis {
    hash: String,
    timestamp: DateTime<Utc>,
    signer: String,
    receiver: String,
    actions: Vec<DecodedAction>,
    total_gas_used: u64,
    patterns_detected: Vec<PatternType>,
    risk_score: u8, // 0-100
    insights: Vec<String>,
}

pub struct TransactionAnalyzerPlugin {
    host: Arc<dyn PluginHost>,
    patterns: Arc<Mutex<HashMap<PatternType, TransactionPattern>>>,
    recent_txs: Arc<Mutex<Vec<TransactionAnalysis>>>,
    config: AnalyzerConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AnalyzerConfig {
    high_value_threshold: u128,    // in yoctoNEAR
    pattern_detection_window: u64, // in seconds
    max_recent_txs: usize,
    risk_thresholds: RiskThresholds,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RiskThresholds {
    gas_spike_multiplier: f64,
    failure_rate_threshold: f64,
    suspicious_pattern_count: u8,
}

impl TransactionAnalyzerPlugin {
    pub fn new(host: Arc<dyn PluginHost>) -> Self {
        Self {
            host,
            patterns: Arc::new(Mutex::new(HashMap::new())),
            recent_txs: Arc::new(Mutex::new(Vec::new())),
            config: AnalyzerConfig {
                high_value_threshold: 1_000_000_000_000_000_000_000_000, // 1 NEAR
                pattern_detection_window: 3600,                          // 1 hour
                max_recent_txs: 1000,
                risk_thresholds: RiskThresholds {
                    gas_spike_multiplier: 2.0,
                    failure_rate_threshold: 0.2,
                    suspicious_pattern_count: 3,
                },
            },
        }
    }

    async fn analyze_transaction(&self, tx: &TxSummary) -> TransactionAnalysis {
        let mut patterns_detected = Vec::new();
        let mut insights = Vec::new();
        let mut risk_score = 0u8;

        // Decode actions
        let mut decoded_actions = Vec::new();
        let mut total_gas = 0u64;

        for action in &tx.actions {
            let decoded = self.decode_action(action).await;
            total_gas += decoded.gas_attached.unwrap_or(0);
            decoded_actions.push(decoded);
        }

        // Pattern detection
        if tx.actions.len() > 3 {
            patterns_detected.push(PatternType::BatchTransaction);
            insights.push("Batch transaction with multiple actions".to_string());
        }

        // Check for high-value transfers
        for action in &decoded_actions {
            if let Some(deposit) = &action.deposit_attached {
                if let Ok(amount) = deposit.parse::<u128>() {
                    if amount > self.config.high_value_threshold {
                        patterns_detected.push(PatternType::HighValueTransfer);
                        insights.push(format!(
                            "High value transfer detected: {} NEAR",
                            amount / 1_000_000_000_000_000_000_000_000
                        ));
                        risk_score += 20;
                    }
                }
            }
        }

        // Check for contract deployment
        if decoded_actions
            .iter()
            .any(|a| a.action_type == "DeployContract")
        {
            patterns_detected.push(PatternType::ContractDeployment);
            insights.push("New contract deployment detected".to_string());
            risk_score += 10;
        }

        // MEV detection (simplified)
        if self.detect_mev_pattern(&tx, &decoded_actions).await {
            patterns_detected.push(PatternType::MEVActivity);
            insights.push("Possible MEV activity detected".to_string());
            risk_score += 30;
        }

        TransactionAnalysis {
            hash: tx.hash.clone(),
            timestamp: Utc::now(),
            signer: tx.signer.clone().unwrap_or_default(),
            receiver: tx.receiver.clone().unwrap_or_default(),
            actions: decoded_actions,
            total_gas_used: total_gas,
            patterns_detected,
            risk_score: risk_score.min(100),
            insights,
        }
    }

    async fn decode_action(&self, action: &TxAction) -> DecodedAction {
        DecodedAction {
            action_type: action.r#type.clone(),
            method_name: action.method.clone(),
            args_decoded: self.try_decode_args(action).await,
            gas_attached: None,     // Would need full tx data
            deposit_attached: None, // Would need full tx data
        }
    }

    async fn try_decode_args(&self, action: &TxAction) -> Option<serde_json::Value> {
        // In a real implementation, this would decode base64 args and parse them
        // For now, return a placeholder
        if action.method.is_some() {
            Some(serde_json::json!({
                "decoded": false,
                "reason": "Decoder not implemented"
            }))
        } else {
            None
        }
    }

    async fn detect_mev_pattern(&self, tx: &TxSummary, actions: &[DecodedAction]) -> bool {
        // Simplified MEV detection logic
        // Real implementation would check for:
        // - Sandwich attacks (buy/sell around user tx)
        // - Arbitrage patterns
        // - Front-running indicators

        let has_swap = actions.iter().any(|a| {
            a.method_name
                .as_ref()
                .map_or(false, |m| m.contains("swap") || m.contains("exchange"))
        });

        let has_multiple_contracts = tx.actions.len() > 1;

        has_swap && has_multiple_contracts
    }

    async fn update_pattern_stats(&self, pattern: PatternType, tx_hash: String) {
        let mut patterns = self.patterns.lock().await;
        let entry = patterns
            .entry(pattern.clone())
            .or_insert(TransactionPattern {
                pattern_type: pattern,
                frequency: 0,
                examples: Vec::new(),
                last_seen: Utc::now(),
            });

        entry.frequency += 1;
        entry.last_seen = Utc::now();
        if entry.examples.len() < 10 {
            entry.examples.push(tx_hash);
        }
    }
}

#[async_trait]
impl Plugin for TransactionAnalyzerPlugin {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            id: "tx-analyzer".to_string(),
            name: "Transaction Analyzer".to_string(),
            version: "0.1.0".to_string(),
            author: "NEARx Team".to_string(),
            description: "Analyzes NEAR transactions for patterns and insights".to_string(),
            capabilities: vec![
                Capability::TransactionAnalysis,
                Capability::CustomQueries,
                Capability::RealtimeUpdates,
            ],
        }
    }

    async fn init(&mut self) -> Result<()> {
        self.host
            .log(LogLevel::Info, "Transaction Analyzer plugin initialized");
        Ok(())
    }

    async fn handle_message(&mut self, message: PluginMessage) -> Result<Option<PluginMessage>> {
        match message {
            PluginMessage::InterestingTransaction {
                hash,
                reason,
                signer,
                receiver,
                actions,
            } => {
                let tx_summary = TxSummary {
                    hash: hash.clone(),
                    signer: Some(signer),
                    receiver: Some(receiver),
                    actions: actions
                        .into_iter()
                        .map(|a| TxAction {
                            r#type: a,
                            method: None,
                        })
                        .collect(),
                };

                let analysis = self.analyze_transaction(&tx_summary).await;

                // Store in recent transactions
                let mut recent = self.recent_txs.lock().await;
                recent.insert(0, analysis.clone());
                if recent.len() > self.config.max_recent_txs {
                    recent.pop();
                }

                // Update pattern statistics
                for pattern in &analysis.patterns_detected {
                    self.update_pattern_stats(pattern.clone(), hash.clone())
                        .await;
                }

                // Log high-risk transactions
                if analysis.risk_score > 70 {
                    self.host.log(
                        LogLevel::Warn,
                        &format!(
                            "High-risk transaction detected: {} (score: {})",
                            hash, analysis.risk_score
                        ),
                    );
                }

                // Return analysis as response
                return Ok(Some(PluginMessage::Response {
                    id: uuid::Uuid::new_v4(),
                    data: serde_json::to_value(analysis)?,
                    success: true,
                    error: None,
                }));
            }

            PluginMessage::Query {
                id,
                query: QueryType::GetRecentTransactions { limit },
            } => {
                let recent = self.recent_txs.lock().await;
                let txs: Vec<_> = recent.iter().take(limit).cloned().collect();

                return Ok(Some(PluginMessage::Response {
                    id,
                    data: serde_json::to_value(txs)?,
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
            .log(LogLevel::Info, "Transaction Analyzer plugin shutting down");
        Ok(())
    }

    fn subscriptions(&self) -> Vec<SubscriptionTopic> {
        vec![
            SubscriptionTopic::AllTransactions,
            SubscriptionTopic::TransactionErrors,
            SubscriptionTopic::HighValueTransactions,
        ]
    }

    async fn tick(&mut self) -> Result<()> {
        // Periodic pattern analysis
        let patterns = self.patterns.lock().await;
        for (pattern_type, pattern_data) in patterns.iter() {
            if pattern_data.frequency > 100 {
                self.host.log(
                    LogLevel::Info,
                    &format!(
                        "Pattern {:?} detected {} times",
                        pattern_type, pattern_data.frequency
                    ),
                );
            }
        }
        Ok(())
    }
}

// Re-export some types that weren't in the core
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxSummary {
    pub hash: String,
    pub signer: Option<String>,
    pub receiver: Option<String>,
    pub actions: Vec<TxAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxAction {
    pub r#type: String,
    pub method: Option<String>,
}
