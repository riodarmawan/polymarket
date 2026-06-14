use crate::config::ProbabilityConfig;

#[derive(Debug, Clone)]
pub struct Signal {
    pub name: String,
    pub value: f64,
    pub confidence: f64,
}

#[derive(Debug, Clone)]
pub struct ProbabilityModel {
    config: ProbabilityConfig,
}

impl ProbabilityModel {
    pub fn new() -> Self {
        Self {
            config: ProbabilityConfig::default(),
        }
    }

    pub fn with_config(config: ProbabilityConfig) -> Self {
        Self { config }
    }

    pub fn calculate(&self, prior: f64, signals: &[Signal]) -> f64 {
        if signals.is_empty() {
            return prior;
        }

        let mut weighted_sum = prior * self.config.prior_weight;
        let mut total_weight = self.config.prior_weight;

        for signal in signals {
            let weight = self.get_signal_weight(&signal.name) * signal.confidence;
            weighted_sum += signal.value * weight;
            total_weight += weight;
        }

        if total_weight > 0.0 {
            (weighted_sum / total_weight).clamp(0.0, 1.0)
        } else {
            prior
        }
    }

    fn get_signal_weight(&self, signal_name: &str) -> f64 {
        match signal_name {
            "news" => self.config.news_weight,
            "polling" => self.config.polling_weight,
            "expert" => self.config.expert_weight,
            "historical" => self.config.historical_weight,
            _ => 0.1,
        }
    }
}

impl Default for ProbabilityModel {
    fn default() -> Self {
        Self::new()
    }
}
