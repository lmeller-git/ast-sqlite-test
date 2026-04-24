use crate::TestOutcome;

pub trait AdaptiveStatistics {
    fn update(&self, test_result: TestOutcome);
    fn calculate_score(&self) -> f64;
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct SchedulerStatisticsSnapshot {
    pub global_attempts: Option<u32>,
    pub name: String,
    pub meta: Vec<String>,
    pub self_attmepts: Vec<u32>,
    pub cov_increases: Vec<u32>,
    pub accepted: Vec<u32>,
    pub synatx_err: Vec<u32>,
    pub crashes: Vec<u32>,
    pub rating: Vec<f64>,
    pub rating_as_prob: Vec<f64>,
}
