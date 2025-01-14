use anyhow::{anyhow, Error};
use runtime_config::KinScoreConfig;

#[derive(Debug)]
pub struct NodePerformanceMetrics {
    pub uptime: f64,                 // Uptime percentage
    pub participation_history: u64,  // Number of blocks proposed/transactions validated
    pub response_time_ms: u64,       // Response time in milliseconds
    pub security_measure_score: f64, // Score based on security measures
}

#[derive(Debug)]
pub struct KinScoreWeights {
    pub weight_for_uptime: f64,                // Weight for Uptime
    pub weight_for_participation_history: f64, // Weight for Active Participation History
    pub weight_for_response_time: f64,         // Weight for Response Time
    pub weight_for_security_measure: f64,      // Weight for Security Measures
}

impl KinScoreWeights {
    pub fn new(config: &KinScoreConfig) -> Self {
        Self {
            weight_for_uptime: config.weight_for_uptime,
            weight_for_participation_history: config.weight_for_participation_history,
            weight_for_response_time: config.weight_for_response_time,
            weight_for_security_measure: config.weight_for_security_measure,
        }
    }

    // Ensure that weights sum up to 1
    pub fn validate_weightage(&self) -> Result<(), Error> {
        if (self.weight_for_uptime
            + self.weight_for_participation_history
            + self.weight_for_response_time
            + self.weight_for_security_measure
            - 1.0)
            .abs()
            > f64::EPSILON
        {
            Err(anyhow!("KinScoreWeights do not sum up to 1"))
        } else {
            Ok(())
        }
    }
}

#[derive(Debug)]
pub struct KinScoreCalculator {
    pub min_uptime: f64,
    pub max_uptime: f64,
    pub min_participation: u32,
    pub max_participation: u32,
    pub min_response_time: u64,
    pub max_response_time: u64,
    pub min_security_measure: f64,
    pub max_security_measure: f64,
    pub weights: KinScoreWeights,
}

impl KinScoreCalculator {
    pub fn new(config: &KinScoreConfig) -> Self {
        Self {
            min_uptime: config.min_uptime,
            max_uptime: config.max_uptime,
            min_participation: config.min_participation, //this will be in %, minimum participation (how many transactions the validator recieved viza vee transactions validated)
            max_participation: config.max_participation,
            min_response_time: config.min_response_time, // time you get message vs the time you process the message
            max_response_time: config.max_response_time,
            min_security_measure: config.min_security_measure,
            max_security_measure: config.max_security_measure,
            weights: KinScoreWeights::new(config),
        }
    }

    pub fn calculate_kin_score(&self, node_metrics: &NodePerformanceMetrics) -> Result<f64, Error> {
        //check for response time make sure if you get below or equal to the minimum response time the code continues to run
        // if you get above the maximum response time the code should return an error
        // //set membership value to one if response time is less than the min_response time
        // if node_metrics.response_time_ms < self.min_response_time || node_metrics.response_time_ms > self.max_response_time {
        //     return Err(anyhow!("Response time out of valid range"));
        // }
        // if node_metrics.security_measure_score < self.min_security_measure || node_metrics.security_measure_score > self.max_security_measure {
        //     return Err(anyhow!("Security measure score out of valid range"));
        // }

        /*let participation_score = self.membership_function(
            node_metrics.participation_history as f64,
            self.min_participation as f64,
            self.max_participation as f64,
        );

        let security_measure_score = self.membership_function(
            node_metrics.security_measure_score,
            self.min_security_measure,
            self.max_security_measure,
        );*/

        let participation_score = 1.0;
        let security_measure_score = 1.0;
        let uptime_score = self.membership_function(node_metrics.uptime, self.min_uptime, self.max_uptime);
        let response_time_score = self.response_time_membership(
            node_metrics.response_time_ms as f64,
            self.min_response_time as f64,
            self.max_response_time as f64,
        );

        Ok(self.weights.weight_for_uptime * uptime_score
            + self.weights.weight_for_participation_history * participation_score
            + self.weights.weight_for_response_time * response_time_score
            + self.weights.weight_for_security_measure * security_measure_score)
    }

    fn membership_function(&self, metric_value: f64, min_value: f64, max_value: f64) -> f64 {
        if metric_value < min_value {
            0.0
        } else if metric_value > max_value {
            1.0
        } else {
            (metric_value - min_value) / (max_value - min_value)
        }
    }

    fn response_time_membership(&self, metric_value: f64, min_value: f64, max_value: f64) -> f64 {
        if metric_value < min_value {
            1.0
        } else if metric_value > max_value {
            0.0
        } else {
            (metric_value - min_value) / (max_value - min_value)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn calculate_kin_score_success() {
        let calculator = KinScoreCalculator::default();
        let node_metrics = NodePerformanceMetrics {
            uptime: 95.0,
            participation_history: 500,
            response_time_ms: 50,
            security_measure_score: 4.75,
        };

        let result = calculator.calculate_kin_score(&node_metrics).await.unwrap();

        assert!(result > 0.0, "KinScore should be positive");
    }

    #[tokio::test]
    async fn calculate_kin_score_error_uptime_out_of_range() {
        let calculator = KinScoreCalculator::default();
        let node_metrics = NodePerformanceMetrics {
            uptime: 49.0, // Below min_uptime
            participation_history: 50,
            response_time_ms: 500,
            security_measure_score: 0.75,
        };

        let result = calculator.calculate_kin_score(&node_metrics).await;
        assert!(result.is_err(), "Expected error due to uptime out of range");
    }

    #[tokio::test]
    async fn calculate_kin_score_error_response_time_out_of_range() {
        let calculator = KinScoreCalculator::default();
        let node_metrics = NodePerformanceMetrics {
            uptime: 75.0,
            participation_history: 50,
            response_time_ms: 9999, // Above max_response_time
            security_measure_score: 0.75,
        };

        let result = calculator.calculate_kin_score(&node_metrics).await;
        assert!(result.is_err(), "Expected error due to response time out of range");
    }

    #[tokio::test]
    async fn calculate_kin_score_error_all_metrics_out_of_range() {
        let calculator = KinScoreCalculator::default();
        let node_metrics = NodePerformanceMetrics {
            uptime: 101.0,               // Above max_uptime
            participation_history: 101,  // Above max_participation
            response_time_ms: 9999,      // Above max_response_time
            security_measure_score: 1.5, // Above max_security_measure
        };

        let result = calculator.calculate_kin_score(&node_metrics).await;
        assert!(result.is_err(), "Expected error due to all metrics out of range");
    }
}
