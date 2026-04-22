use crate::TestOutcome;

pub trait AdaptiveStatistics {
    fn update(&self, test_result: TestOutcome);
    fn calculate_score(&self) -> f64;
}
