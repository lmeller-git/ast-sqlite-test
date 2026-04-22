use crate::TestOutcome;

pub trait FeedbackHook: Send + Sync {
    fn fire(&self, test_outcome: TestOutcome);
}
