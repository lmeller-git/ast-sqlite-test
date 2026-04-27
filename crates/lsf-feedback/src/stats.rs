use crate::TestOutcome;

pub trait AdaptiveStatistics {
    fn update(&self, test_result: TestOutcome);
    fn calculate_score(&self) -> f64;
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct SchedulerStatisticsSnapshot {
    pub global_attempts: Option<f64>,
    pub name: String,
    pub meta: Vec<String>,
    pub self_attmepts: Vec<f64>,
    pub cov_increases: Vec<f64>,
    pub accepted: Vec<f64>,
    pub synatx_err: Vec<f64>,
    pub crashes: Vec<f64>,
    pub rating: Vec<f64>,
    pub rating_as_prob: Vec<f64>,
}
