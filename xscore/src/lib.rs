pub mod kin_score;
pub mod stake_score;
use anyhow::Error;
use kin_score::{KinScoreCalculator, NodePerformanceMetrics};
use runtime_config::{StakeScoreConfig, XscoreConfig};
use stake_score::{NodeStakeInfo, StakeScoreCalculator};

// XScoreWeights struct to hold the weights for StakeScore and KinScore
#[derive(Debug)]
pub struct XScoreWeights {
    pub weight_for_stakescore: f64, // Weight for StakeScore
    pub weight_for_kinscore: f64,   // Weight for KinScore
}

impl XScoreWeights {
    pub fn new(config: &XscoreConfig) -> Self {
        Self {
            weight_for_stakescore: config.weight_for_stakescore,
            weight_for_kinscore: config.weight_for_kinscore,
        }
    }

    pub fn validate_weightage(&self) -> Result<(), Error> {
        if (self.weight_for_stakescore + self.weight_for_kinscore - 1.0).abs() > f64::EPSILON {
            Err(anyhow::anyhow!("StakeScoreWeights do not sum up to 1"))
        } else {
            Ok(())
        }
    }
}
// XScoreCalculator struct that will use both KinScore and StakeScore calculators
#[derive(Debug)]
pub struct XScoreCalculator {
    pub stake_score_calculator: StakeScoreCalculator,
    pub kin_score_calculator: KinScoreCalculator,
    pub weights: XScoreWeights,
    pub xscore_threshold: f64, // The XScore threshold for participation eligibility
}

impl XScoreCalculator {
    pub fn calculate_xscore(
        &self,
        node_stake_info: &NodeStakeInfo,
        node_performance_metrics: &NodePerformanceMetrics,
        stake_score_config: &StakeScoreConfig,
    ) -> Result<f64, String> {
        // Validate weights before calculation
        if self.stake_score_calculator.weights.validate_weightage().is_err() {
            return Ok(0.0);
        };
        let stake_score = self
            .stake_score_calculator
            .calculate_stake_score(node_stake_info, stake_score_config)
            .map_err(|e| e.to_string())?;
        // Validate weights before calculation
        self.kin_score_calculator
            .weights
            .validate_weightage()
            .map_err(|e| e.to_string())?;
        let kin_score = self
            .kin_score_calculator
            .calculate_kin_score(node_performance_metrics)
            .map_err(|e| e.to_string())?;
        if self.weights.validate_weightage().is_err() {
            return Ok(0.0);
        };

        let xscore = self.weights.weight_for_stakescore * stake_score + self.weights.weight_for_kinscore * kin_score;
        Ok(xscore)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper function to create a NodeStakeInfo structure
    fn create_stake_info(balance: u64, age: u64, locking_period: u64) -> NodeStakeInfo {
        NodeStakeInfo {
            stake_balance: balance as u128,
            stake_age_days: age,
            locking_period_days: locking_period,
        }
    }

    // Helper function to create a NodePerformanceMetrics structure
    fn create_performance_metrics(
        uptime: f64,
        history: u64,
        response_time: u64,
        security_score: f64,
    ) -> NodePerformanceMetrics {
        NodePerformanceMetrics {
            uptime,
            participation_history: history,
            response_time_ms: response_time,
            security_measure_score: security_score,
        }
    }
    #[tokio::test]
    async fn test_xscore_integration() {
        // Setup test environment and dependencies
        let stake_info = NodeStakeInfo {
            stake_balance: 100000,    // Measured in tokens
            stake_age_days: 365,      // Measured in months
            locking_period_days: 862, // Measured in months
        };
        let performance_metrics = NodePerformanceMetrics {
            uptime: 100.0,               // Percentage
            participation_history: 300,  // Number of blocks participated
            response_time_ms: 5,         // Measured in milliseconds
            security_measure_score: 9.5, // Rating out of 10
        };

        // Instantiate xscore calculator with predefined weights
        let calculator = XScoreCalculator {
            stake_score_calculator: Default::default(),
            kin_score_calculator: Default::default(),
            weights: Default::default(),
            xscore_threshold: 0.0,
        };

        // Calculate xscore
        let xscore = calculator.calculate_xscore(&stake_info, &performance_metrics).await.unwrap();
        let stake_score = calculator
            .stake_score_calculator
            .calculate_stake_score(&stake_info)
            .await
            .map_err(|e| e.to_string())
            .unwrap();
        let kin_score = calculator
            .kin_score_calculator
            .calculate_kin_score(&performance_metrics)
            .await
            .map_err(|e| e.to_string())
            .unwrap();

        let expected_xscore = 0.5 * stake_score + 0.5 * kin_score;

        // Assertions to validate xscore against expected values
        assert_eq!(
            xscore, expected_xscore,
            "XScore calculation does not match expected value."
        );
    }

    #[tokio::test]
    async fn test_xscore_minimum_values() {
        let stake_info = create_stake_info(0, 0, 0);
        let performance_metrics = create_performance_metrics(0.0, 0, u64::MAX, 0.0);

        let calculator = XScoreCalculator {
            weights: XScoreWeights {
                weight_for_stakescore: 0.5,
                weight_for_kinscore: 0.5,
            },
            ..Default::default()
        };

        let xscore = match calculator.calculate_xscore(&stake_info, &performance_metrics).await {
            Ok(x) => x,
            Err(_) => 0.0,
        };
        assert_eq!(xscore, 0.0, "XScore should be 0 for minimum input values.");
    }
    #[tokio::test]
    async fn test_xscore_maximum_values() {
        let stake_info = create_stake_info(u64::MAX, 360000000000, 360000000000);
        let performance_metrics = create_performance_metrics(100.0, u64::MAX, 1, 5.0);

        let calculator = XScoreCalculator {
            weights: XScoreWeights {
                weight_for_stakescore: 0.5,
                weight_for_kinscore: 0.5,
            },
            ..Default::default()
        };

        let xscore = calculator.calculate_xscore(&stake_info, &performance_metrics).await.unwrap();
        assert!(xscore == 1.0, "XScore should be positive for maximum input values.");
    }

    #[tokio::test]
    async fn test_xscore_edge_cases() {
        let stake_info = create_stake_info(50000, 240, 120);
        let performance_metrics_zero_response = create_performance_metrics(85.0, 100, 0, 3.5); // Edge case: zero response time
        let performance_metrics_max_uptime = create_performance_metrics(100.0, 100, 100, 3.5); // Edge case: 100% uptime

        let calculator = XScoreCalculator {
            weights: XScoreWeights {
                weight_for_stakescore: 0.5,
                weight_for_kinscore: 0.5,
            },
            ..Default::default()
        };

        let xscore_zero_response = calculator
            .calculate_xscore(&stake_info, &performance_metrics_zero_response)
            .await
            .unwrap();
        assert!(xscore_zero_response > 0.0, "XScore should handle zero response time.");

        let xscore_max_uptime = calculator
            .calculate_xscore(&stake_info, &performance_metrics_max_uptime)
            .await
            .unwrap();
        assert!(xscore_max_uptime > 0.0, "XScore should be positive with 100% uptime.");
    }

    #[tokio::test]
    async fn test_xscore_calculation_above_threshold() {
        // Setup default calculators and weights
        let stake_score_calculator = StakeScoreCalculator::default();
        let kin_score_calculator = KinScoreCalculator::default();
        let xscore_calculator = XScoreCalculator {
            stake_score_calculator,
            kin_score_calculator,
            weights: XScoreWeights {
                weight_for_stakescore: 0.5, // Example weight for StakeScore
                weight_for_kinscore: 0.5,   // Example weight for KinScore
            },
            xscore_threshold: 0.24, // Example threshold
        };

        // Example node info for testing
        let node_stake_info = NodeStakeInfo {
            stake_balance: 50000,
            stake_age_days: 365,
            locking_period_days: 730,
        };
        let node_performance_metrics = NodePerformanceMetrics {
            uptime: 99.0,
            participation_history: 200,
            response_time_ms: 100,
            security_measure_score: 1.0,
        };

        // Calculate XScore
        let xscore_result = xscore_calculator
            .calculate_xscore(&node_stake_info, &node_performance_metrics)
            .await;
        assert!(xscore_result.is_ok(), "Expected XScore calculation to succeed");
        let xscore = xscore_result.unwrap();
        assert!(
            xscore > xscore_calculator.xscore_threshold,
            "Expected XScore to be above the threshold"
        );
    }

    #[tokio::test]
    async fn test_xscore_calculation_below_threshold() {
        // Setup default calculators and weights
        let stake_score_calculator = StakeScoreCalculator::default();
        let kin_score_calculator = KinScoreCalculator::default();
        let xscore_calculator = XScoreCalculator {
            stake_score_calculator,
            kin_score_calculator,
            weights: XScoreWeights {
                weight_for_stakescore: 0.5, // Example weight for StakeScore
                weight_for_kinscore: 0.5,   // Example weight for KinScore
            },
            xscore_threshold: 0.8, // Example threshold
        };

        // Example node info for testing
        let node_stake_info = NodeStakeInfo {
            stake_balance: 50000,
            stake_age_days: 365,
            locking_period_days: 730,
        };
        let node_performance_metrics = NodePerformanceMetrics {
            uptime: 99.0,
            participation_history: 200,
            response_time_ms: 100,
            security_measure_score: 1.0,
        };

        // Calculate XScore
        let xscore_result = xscore_calculator
            .calculate_xscore(&node_stake_info, &node_performance_metrics)
            .await
            .unwrap();

        assert!(
            xscore_result < xscore_calculator.xscore_threshold,
            "Expected XScore calculation to fail"
        );
    }
}
