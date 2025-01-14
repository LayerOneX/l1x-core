use anyhow::Error;
use primitives::Balance;
use runtime_config::StakeScoreConfig;

#[derive(Debug, Default)]
pub struct NodeStakeInfo {
    pub stake_balance: Balance,
    pub stake_age_days: u64, // Age in days
    pub locking_period_days: u64,
}

#[derive(Debug)]
pub struct StakeScoreWeights {
    pub weight_for_stake_balance: f64,  // Weight for Stake Balance
    pub weight_for_stake_age: f64,      // Weight for Stake Age
    pub weight_for_locking_period: f64, // Weight for Locking Period
}

impl StakeScoreWeights {
    // Adjust weights based on the time period in months
    // Adjust these to come from the database
    pub fn new_weights(time_period_months: u8, config: &StakeScoreConfig) -> Self {
        match time_period_months {
            2..=6 => Self {
                // Assume these are the weights for 2-6 months
                weight_for_stake_balance: config.weight_for_stake_balance_6,
                weight_for_stake_age: config.weight_for_stake_age_6,
                weight_for_locking_period: config.weight_for_locking_period_6,
            },
            7..=12 => Self {
                // Assume these are the weights for 7-12 months
                weight_for_stake_balance: config.weight_for_stake_balance_12,
                weight_for_stake_age: config.weight_for_stake_age_12,
                weight_for_locking_period: config.weight_for_locking_period_12,
            },
            13..=18 => Self {
                // Assume these are the weights for 13-18 months
                weight_for_stake_balance: config.weight_for_stake_balance_18,
                weight_for_stake_age: config.weight_for_stake_age_18,
                weight_for_locking_period: config.weight_for_locking_period_18,
            },
            _ => Self {
                // Default case for other time periods
                weight_for_stake_balance: config.weight_for_stake_balance_12,
                weight_for_stake_age: config.weight_for_stake_age_12,
                weight_for_locking_period: config.weight_for_locking_period_12,
            },
        }
    }
}

impl StakeScoreWeights {
    // Ensure that weights sum up to 1
    pub fn validate_weightage(&self) -> Result<(), Error> {
        if (self.weight_for_stake_balance + self.weight_for_stake_age + self.weight_for_locking_period - 1.0).abs()
            > f64::EPSILON
        {
            Err(anyhow::anyhow!("StakeScoreWeights do not sum up to 1"))
        } else {
            Ok(())
        }
    }
}
//TODO: add a base stake score, this is based on the l1x node nft
#[derive(Debug)]
pub struct StakeScoreCalculator {
    pub min_balance: u128,
    pub max_balance: u128,
    pub min_stake_age: u64,
    pub max_stake_age: u64,
    pub min_lock_period: u64,
    pub max_lock_period: u64,
    pub weights: StakeScoreWeights,
}

impl StakeScoreCalculator {
    pub fn new(time_period_months: u8, config: &StakeScoreConfig) -> Self {
        Self {
            min_balance: config.min_balance as u128,
            max_balance: config.max_balance as u128,
            min_stake_age: config.min_stake_age,
            max_stake_age: config.max_stake_age,
            min_lock_period: config.min_lock_period,
            max_lock_period: config.max_lock_period,
            weights: StakeScoreWeights::new_weights(time_period_months, config),
        }
    }
    // Calculate the stake score of the node
    pub fn calculate_stake_score(&self, node: &NodeStakeInfo, config: &StakeScoreConfig) -> Result<f64, Error> {
        let time_period_months = node.stake_age_days as f32 / 30.44f32;
        let stake_calculator = StakeScoreCalculator::new(time_period_months as u8, config);
        if node.stake_balance < self.min_balance {
            return Ok(0.0);
        }

        let stake_age_score = self.membership_function(
            node.stake_age_days as f64,
            self.min_stake_age as f64,
            self.max_stake_age as f64,
        );

        let stake_balance_score = self.membership_function(
            node.stake_balance as f64,
            self.min_balance as f64,
            self.max_balance as f64,
        );
        let locking_period_score = 1.0;

        // let locking_period_score = self.membership_function(
        //     node.locking_period_days as f64,
        //     self.min_lock_period as f64,
        //     self.max_lock_period as f64,
        // );

        // Weighted arithmetic mean calculation
        Ok(stake_calculator.weights.weight_for_stake_balance * stake_balance_score
            + stake_calculator.weights.weight_for_stake_age * stake_age_score
            + stake_calculator.weights.weight_for_locking_period * locking_period_score)
    }

    fn membership_function(&self, value: f64, min: f64, max: f64) -> f64 {
        // This already ensures a return within [0.0, 1.0]
        if value < min {
            0.0
        } else if value > max {
            1.0
        } else {
            (value - min) / (max - min)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_valid_stake_score() {
        let calculator = StakeScoreCalculator::default();

        let node_info = NodeStakeInfo {
            stake_balance: 500000,
            stake_age_days: 1800,      // 1 year
            locking_period_days: 3650, // 6 months
        };

        let score = calculator.calculate_stake_score(&node_info).await.unwrap();
        assert!(score > 0.0, "StakeScore should be greater than 0");
    }

    #[tokio::test]
    async fn test_stake_score_with_invalid_balance() {
        let mut calculator = StakeScoreCalculator::default();
        calculator.weights.weight_for_stake_balance = 0.4;
        calculator.weights.weight_for_stake_age = 0.4;
        calculator.weights.weight_for_locking_period = 0.3;
        let node_info = NodeStakeInfo {
            stake_balance: 500, // Below min_balance
            stake_age_days: 365,
            locking_period_days: 180,
        };

        let result = calculator.calculate_stake_score(&node_info).await;

        assert!(
            result.is_err(),
            "Invalid balance should be greater than minimum balance"
        );
    }

    #[tokio::test]
    async fn test_stake_score_with_invalid_stake_age() {
        // Setup calculator with specific ranges for testing
        let mut calculator = StakeScoreCalculator::default();
        calculator.weights.weight_for_stake_balance = 0.4;
        calculator.weights.weight_for_stake_age = 0.4;
        calculator.weights.weight_for_locking_period = 0.3;

        let node_info = NodeStakeInfo {
            stake_balance: 5000,
            stake_age_days: 29, // Below min_stake_age
            locking_period_days: 180,
        };

        let result = calculator.calculate_stake_score(&node_info).await;
        assert!(
            result.is_err(),
            "Invalid stake age should be greater or equal to minimum stake age"
        );
    }

    #[tokio::test]
    async fn test_stake_score_with_invalid_locking_period() {
        // Setup calculator with specific ranges for testing
        let mut calculator = StakeScoreCalculator::default();
        calculator.weights.weight_for_stake_balance = 0.4;
        calculator.weights.weight_for_stake_age = 0.4;
        calculator.weights.weight_for_locking_period = 0.3;

        let node_info = NodeStakeInfo {
            stake_balance: 5000,
            stake_age_days: 365,
            locking_period_days: 89, // Below min_lock_period
        };

        let result = calculator.calculate_stake_score(&node_info).await;
        assert!(
            result.is_err(),
            "Invalid locking period should be greater or equal to minimum locking period"
        );
    }
}
