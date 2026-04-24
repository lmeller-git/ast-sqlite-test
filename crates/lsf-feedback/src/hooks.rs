use std::sync::Arc;

use crate::{SchedulerStatisticsSnapshot, TestOutcome};

pub trait FeedbackHook: Send + Sync {
    fn fire(&self, test_outcome: TestOutcome);
}

pub trait Hookable {
    fn snapshot(&self) -> SchedulerStatisticsSnapshot;
    fn attach_hook(&mut self, hook: Arc<dyn GenericHook>);
}

pub trait GenericHook: Send + Sync {
    fn on_snapshot(&self, snapshot: SchedulerStatisticsSnapshot);
}
