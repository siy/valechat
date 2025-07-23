use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use rust_decimal::Decimal;
use tracing::{debug, info};

use crate::error::Result;
use crate::models::provider::{ModelProvider, ModelCapabilities, PricingInfo};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRequirements {
    pub task_type: TaskType,
    pub max_tokens_needed: Option<u32>,
    pub requires_streaming: bool,
    pub requires_function_calling: bool,
    pub requires_vision: bool,
    pub max_cost_per_request: Option<Decimal>,
    pub max_response_time_ms: Option<u64>,
    pub quality_priority: QualityPriority,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum TaskType {
    ConversationalChat,
    CodeGeneration,
    DocumentAnalysis,
    CreativeWriting,
    ReasoningAndAnalysis,
    Translation,
    Summarization,
    QuestionAnswering,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum QualityPriority {
    Speed,     // Prioritize fastest response
    Cost,      // Prioritize lowest cost
    Quality,   // Prioritize best quality/capability
    Balanced,  // Balance all factors
}

#[derive(Debug, Clone)]
pub struct ModelRecommendation {
    pub provider_name: String,
    pub model_name: String,
    pub confidence_score: f32, // 0.0 to 1.0
    pub estimated_cost: Decimal,
    pub estimated_response_time_ms: u64,
    pub capability_match: f32,
    pub reasoning: String,
}

pub struct CapabilityDetector {
    providers: HashMap<String, Box<dyn ModelProvider + Send + Sync>>,
    model_performance_data: HashMap<String, ModelPerformanceData>,
}

#[derive(Debug, Clone)]
struct ModelPerformanceData {
    avg_response_time_ms: u64,
    success_rate: f32,
    avg_quality_score: f32,
    #[allow(dead_code)]
    cost_effectiveness: f32,
}

impl CapabilityDetector {
    pub fn new() -> Self {
        let mut detector = Self {
            providers: HashMap::new(),
            model_performance_data: HashMap::new(),
        };
        
        // Initialize with default performance data
        detector.initialize_performance_data();
        detector
    }

    pub fn add_provider(&mut self, provider: Box<dyn ModelProvider + Send + Sync>) -> Result<()> {
        let provider_name = provider.get_provider_name().to_string();
        debug!("Adding provider: {}", provider_name);
        
        self.providers.insert(provider_name, provider);
        Ok(())
    }

    pub async fn recommend_model(&self, requirements: &TaskRequirements) -> Result<Vec<ModelRecommendation>> {
        info!("Analyzing requirements for task type: {:?}", requirements.task_type);
        
        let mut recommendations = Vec::new();
        
        for (provider_name, provider) in &self.providers {
            if let Ok(health) = provider.health_check().await {
                if !health.is_healthy {
                    debug!("Skipping unhealthy provider: {}", provider_name);
                    continue;
                }
            }
            
            let capabilities = provider.get_capabilities();
            let pricing = provider.get_pricing();
            
            if let Some(recommendation) = self.evaluate_provider_for_task(
                provider_name,
                &capabilities,
                pricing.as_ref(),
                requirements,
            ).await {
                recommendations.push(recommendation);
            }
        }
        
        // Sort by confidence score (descending)
        recommendations.sort_by(|a, b| b.confidence_score.partial_cmp(&a.confidence_score).unwrap());
        
        info!("Generated {} recommendations", recommendations.len());
        Ok(recommendations)
    }

    async fn evaluate_provider_for_task(
        &self,
        provider_name: &str,
        capabilities: &ModelCapabilities,
        pricing: Option<&PricingInfo>,
        requirements: &TaskRequirements,
    ) -> Option<ModelRecommendation> {
        let model_name = self.get_best_model_for_provider(provider_name, requirements);
        let full_model_key = format!("{}:{}", provider_name, model_name);
        
        // Check basic capability requirements
        let capability_match = self.calculate_capability_match(capabilities, requirements);
        if capability_match < 0.3 {
            debug!("Provider {} has low capability match: {}", provider_name, capability_match);
            return None;
        }
        
        // Calculate estimated cost
        let estimated_cost = if let Some(pricing_info) = pricing {
            let estimated_tokens = requirements.max_tokens_needed.unwrap_or(1000);
            pricing_info.calculate_cost(&crate::models::provider::TokenUsage::new(
                estimated_tokens / 2, // Rough estimate of input tokens
                estimated_tokens / 2, // Rough estimate of output tokens
            ))
        } else {
            Decimal::ZERO
        };
        
        // Check cost constraint
        if let Some(max_cost) = requirements.max_cost_per_request {
            if estimated_cost > max_cost {
                debug!("Provider {} exceeds cost limit: {} > {}", provider_name, estimated_cost, max_cost);
                return None;
            }
        }
        
        // Get performance data
        let performance = self.model_performance_data
            .get(&full_model_key)
            .cloned()
            .unwrap_or_else(|| self.get_default_performance_data(provider_name));
        
        // Check response time constraint
        if let Some(max_time) = requirements.max_response_time_ms {
            if performance.avg_response_time_ms > max_time {
                debug!("Provider {} exceeds time limit: {} > {}", provider_name, performance.avg_response_time_ms, max_time);
                return None;
            }
        }
        
        // Calculate confidence score based on priority
        let confidence_score = self.calculate_confidence_score(
            &requirements.quality_priority,
            capability_match,
            estimated_cost,
            performance.avg_response_time_ms,
            performance.success_rate,
            performance.avg_quality_score,
        );
        
        let reasoning = self.generate_reasoning(
            provider_name,
            &model_name,
            capability_match,
            estimated_cost,
            performance.avg_response_time_ms,
            requirements,
        );
        
        Some(ModelRecommendation {
            provider_name: provider_name.to_string(),
            model_name,
            confidence_score,
            estimated_cost,
            estimated_response_time_ms: performance.avg_response_time_ms,
            capability_match,
            reasoning,
        })
    }

    fn calculate_capability_match(&self, capabilities: &ModelCapabilities, requirements: &TaskRequirements) -> f32 {
        let mut score = 0.0;
        let mut total_checks = 0.0;
        
        // Check streaming requirement
        total_checks += 1.0;
        if requirements.requires_streaming {
            if capabilities.supports_streaming {
                score += 1.0;
            } else {
                return 0.0; // Hard requirement
            }
        } else {
            score += 1.0; // No requirement, so it passes
        }
        
        // Check function calling requirement
        total_checks += 1.0;
        if requirements.requires_function_calling {
            if capabilities.supports_function_calling {
                score += 1.0;
            } else {
                return 0.0; // Hard requirement
            }
        } else {
            score += 1.0;
        }
        
        // Check vision requirement
        total_checks += 1.0;
        if requirements.requires_vision {
            if capabilities.supports_vision {
                score += 1.0;
            } else {
                return 0.0; // Hard requirement
            }
        } else {
            score += 1.0;
        }
        
        // Check token requirements
        total_checks += 1.0;
        if let Some(needed_tokens) = requirements.max_tokens_needed {
            if capabilities.max_tokens >= needed_tokens {
                score += 1.0;
            } else {
                score += (capabilities.max_tokens as f32) / (needed_tokens as f32);
            }
        } else {
            score += 1.0;
        }
        
        // Check context window
        total_checks += 1.0;
        let needed_context = requirements.max_tokens_needed.unwrap_or(4000) * 3; // Rough estimate
        if capabilities.context_window >= needed_context {
            score += 1.0;
        } else {
            score += (capabilities.context_window as f32) / (needed_context as f32);
        }
        
        score / total_checks
    }

    fn calculate_confidence_score(
        &self,
        priority: &QualityPriority,
        capability_match: f32,
        estimated_cost: Decimal,
        avg_response_time: u64,
        success_rate: f32,
        quality_score: f32,
    ) -> f32 {
        let cost_score = if estimated_cost > Decimal::ZERO {
            // Normalize cost score (lower cost = higher score)
            let cost_float = estimated_cost.to_string().parse::<f32>().unwrap_or(1.0);
            (1.0 / (1.0 + cost_float * 100.0)).max(0.1)
        } else {
            0.8 // Default for unknown cost
        };
        
        let speed_score = {
            // Normalize speed score (faster = higher score)
            let time_seconds = avg_response_time as f32 / 1000.0;
            (1.0 / (1.0 + time_seconds / 10.0)).max(0.1)
        };
        
        match priority {
            QualityPriority::Speed => {
                0.1 * capability_match + 0.1 * cost_score + 0.6 * speed_score + 0.2 * success_rate
            }
            QualityPriority::Cost => {
                0.1 * capability_match + 0.6 * cost_score + 0.1 * speed_score + 0.2 * success_rate
            }
            QualityPriority::Quality => {
                0.3 * capability_match + 0.1 * cost_score + 0.1 * speed_score + 0.5 * quality_score
            }
            QualityPriority::Balanced => {
                0.25 * capability_match + 0.25 * cost_score + 0.25 * speed_score + 0.25 * success_rate
            }
        }
    }

    fn get_best_model_for_provider(&self, provider_name: &str, requirements: &TaskRequirements) -> String {
        match provider_name {
            "openai" => match requirements.task_type {
                TaskType::CodeGeneration => "gpt-4".to_string(),
                TaskType::ReasoningAndAnalysis => "gpt-4".to_string(),
                TaskType::ConversationalChat if matches!(requirements.quality_priority, QualityPriority::Speed) => {
                    "gpt-3.5-turbo".to_string()
                }
                _ => "gpt-4".to_string(),
            },
            "anthropic" => match requirements.task_type {
                TaskType::CodeGeneration => "claude-3-5-sonnet-20241022".to_string(),
                TaskType::ReasoningAndAnalysis => "claude-3-opus-20240229".to_string(),
                TaskType::ConversationalChat if matches!(requirements.quality_priority, QualityPriority::Speed | QualityPriority::Cost) => {
                    "claude-3-haiku-20240307".to_string()
                }
                _ => "claude-3-sonnet-20240229".to_string(),
            },
            "gemini" => match requirements.task_type {
                TaskType::ConversationalChat if matches!(requirements.quality_priority, QualityPriority::Speed | QualityPriority::Cost) => {
                    "gemini-1.5-flash".to_string()
                }
                _ => "gemini-1.5-pro".to_string(),
            },
            _ => "default".to_string(),
        }
    }

    fn generate_reasoning(
        &self,
        provider_name: &str,
        model_name: &str,
        capability_match: f32,
        estimated_cost: Decimal,
        avg_response_time: u64,
        requirements: &TaskRequirements,
    ) -> String {
        let mut reasons = Vec::new();
        
        reasons.push(format!("Capability match: {:.0}%", capability_match * 100.0));
        
        if estimated_cost > Decimal::ZERO {
            reasons.push(format!("Estimated cost: ${:.4}", estimated_cost));
        }
        
        reasons.push(format!("Expected response time: {}ms", avg_response_time));
        
        match requirements.quality_priority {
            QualityPriority::Speed => reasons.push("Optimized for speed".to_string()),
            QualityPriority::Cost => reasons.push("Optimized for cost efficiency".to_string()),
            QualityPriority::Quality => reasons.push("Optimized for quality".to_string()),
            QualityPriority::Balanced => reasons.push("Balanced optimization".to_string()),
        }
        
        format!("{} {} - {}", provider_name, model_name, reasons.join(", "))
    }

    fn initialize_performance_data(&mut self) {
        // OpenAI models
        self.model_performance_data.insert("openai:gpt-4".to_string(), ModelPerformanceData {
            avg_response_time_ms: 3000,
            success_rate: 0.98,
            avg_quality_score: 0.95,
            cost_effectiveness: 0.7,
        });
        
        self.model_performance_data.insert("openai:gpt-3.5-turbo".to_string(), ModelPerformanceData {
            avg_response_time_ms: 1500,
            success_rate: 0.96,
            avg_quality_score: 0.85,
            cost_effectiveness: 0.9,
        });
        
        // Anthropic models
        self.model_performance_data.insert("anthropic:claude-3-opus-20240229".to_string(), ModelPerformanceData {
            avg_response_time_ms: 4000,
            success_rate: 0.97,
            avg_quality_score: 0.96,
            cost_effectiveness: 0.6,
        });
        
        self.model_performance_data.insert("anthropic:claude-3-sonnet-20240229".to_string(), ModelPerformanceData {
            avg_response_time_ms: 2500,
            success_rate: 0.96,
            avg_quality_score: 0.90,
            cost_effectiveness: 0.8,
        });
        
        self.model_performance_data.insert("anthropic:claude-3-haiku-20240307".to_string(), ModelPerformanceData {
            avg_response_time_ms: 1200,
            success_rate: 0.94,
            avg_quality_score: 0.82,
            cost_effectiveness: 0.95,
        });
        
        // Gemini models
        self.model_performance_data.insert("gemini:gemini-1.5-pro".to_string(), ModelPerformanceData {
            avg_response_time_ms: 2800,
            success_rate: 0.95,
            avg_quality_score: 0.88,
            cost_effectiveness: 0.85,
        });
        
        self.model_performance_data.insert("gemini:gemini-1.5-flash".to_string(), ModelPerformanceData {
            avg_response_time_ms: 1000,
            success_rate: 0.93,
            avg_quality_score: 0.80,
            cost_effectiveness: 0.98,
        });
    }

    fn get_default_performance_data(&self, provider_name: &str) -> ModelPerformanceData {
        match provider_name {
            "openai" => ModelPerformanceData {
                avg_response_time_ms: 2000,
                success_rate: 0.95,
                avg_quality_score: 0.85,
                cost_effectiveness: 0.8,
            },
            "anthropic" => ModelPerformanceData {
                avg_response_time_ms: 2500,
                success_rate: 0.95,
                avg_quality_score: 0.88,
                cost_effectiveness: 0.75,
            },
            "gemini" => ModelPerformanceData {
                avg_response_time_ms: 2000,
                success_rate: 0.93,
                avg_quality_score: 0.82,
                cost_effectiveness: 0.9,
            },
            _ => ModelPerformanceData {
                avg_response_time_ms: 3000,
                success_rate: 0.9,
                avg_quality_score: 0.8,
                cost_effectiveness: 0.7,
            },
        }
    }
}

impl Default for CapabilityDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::prelude::*;

    #[test]
    fn test_task_requirements_creation() {
        let requirements = TaskRequirements {
            task_type: TaskType::CodeGeneration,
            max_tokens_needed: Some(2000),
            requires_streaming: false,
            requires_function_calling: false,
            requires_vision: false,
            max_cost_per_request: Some(Decimal::from_str("0.10").unwrap()),
            max_response_time_ms: Some(5000),
            quality_priority: QualityPriority::Quality,
        };
        
        assert_eq!(requirements.task_type, TaskType::CodeGeneration);
        assert_eq!(requirements.quality_priority, QualityPriority::Quality);
    }

    #[test]
    fn test_capability_detector_creation() {
        let detector = CapabilityDetector::new();
        assert!(!detector.model_performance_data.is_empty());
    }

    #[test]
    fn test_model_selection_for_provider() {
        let detector = CapabilityDetector::new();
        
        let code_requirements = TaskRequirements {
            task_type: TaskType::CodeGeneration,
            max_tokens_needed: Some(4000),
            requires_streaming: false,
            requires_function_calling: false,
            requires_vision: false,
            max_cost_per_request: None,
            max_response_time_ms: None,
            quality_priority: QualityPriority::Quality,
        };
        
        let model = detector.get_best_model_for_provider("openai", &code_requirements);
        assert_eq!(model, "gpt-4");
        
        let model = detector.get_best_model_for_provider("anthropic", &code_requirements);
        assert_eq!(model, "claude-3-5-sonnet-20241022");
    }

    #[test]
    fn test_confidence_score_calculation() {
        let detector = CapabilityDetector::new();
        
        let score = detector.calculate_confidence_score(
            &QualityPriority::Speed,
            0.9,  // capability_match
            Decimal::from_str("0.01").unwrap(), // estimated_cost
            1000, // avg_response_time
            0.95, // success_rate
            0.85, // quality_score
        );
        
        assert!(score > 0.0 && score <= 1.0);
    }
}