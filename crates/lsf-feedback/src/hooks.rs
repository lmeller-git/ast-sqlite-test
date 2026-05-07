use lsf_core::entry::Meta;

use crate::TestOutcome;

pub trait FeedbackHook: Send + Sync {
    fn on_exec(&self, _test_outcome: TestOutcome, _meta: &Meta) {}
    fn on_mutate(&self, _mutation_outcome: TestOutcome) {}
}
