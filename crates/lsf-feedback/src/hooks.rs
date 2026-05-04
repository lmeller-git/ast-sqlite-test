use crate::TestOutcome;

pub trait FeedbackHook: Send + Sync {
    fn on_exec(&self, _test_outcome: TestOutcome) {}
    fn on_mutate(&self, _mutation_outcome: TestOutcome) {}
}
