use std::{
    fmt::Debug,
    ops::{Deref, DerefMut},
    sync::Arc,
};

mod hooks;
mod stats;
pub use hooks::*;
use lsf_core::entry::RawEntry;
pub use stats::*;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum TestOutcome {
    Rejected(RejectionReason),
    Accepted(AcceptanceReason),
    Mutated,
    NOOP,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum RejectionReason {
    SyntaxError,
    TriggersCrash,
    Bad,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum AcceptanceReason {
    CovIncrease(usize),
    IsDiverse,
}

#[derive(Clone)]
pub struct TestableEntry<T> {
    entry: T,
    pub hooks: Vec<Arc<dyn FeedbackHook>>,
    pub build_hooks: Vec<Arc<dyn FeedbackHook>>,
}

impl<T> TestableEntry<T> {
    pub fn new(entry: T) -> Self {
        Self {
            entry,
            hooks: Vec::new(),
            build_hooks: Vec::new(),
        }
    }

    pub fn attach_hook(&mut self, hook: Arc<dyn FeedbackHook>) {
        self.hooks.push(hook);
    }

    pub fn with_hook(mut self, hook: Arc<dyn FeedbackHook>) -> Self {
        self.attach_hook(hook);
        self
    }

    pub fn attach_build_hook(&mut self, hook: Arc<dyn FeedbackHook>) {
        self.build_hooks.push(hook);
    }

    pub fn with_build_hook(mut self, hook: Arc<dyn FeedbackHook>) -> Self {
        self.attach_build_hook(hook);
        self
    }

    pub fn fire_hooks(&self, outcome: TestOutcome) {
        for hook in &self.hooks {
            hook.fire(outcome);
        }
    }

    pub fn fire_build_hooks(&self, outcome: TestOutcome) {
        for hook in &self.build_hooks {
            hook.fire(outcome);
        }
    }
}

impl<T> AsRef<T> for TestableEntry<T> {
    fn as_ref(&self) -> &T {
        self
    }
}

impl<T> AsMut<T> for TestableEntry<T> {
    fn as_mut(&mut self) -> &mut T {
        self
    }
}

impl<T> Deref for TestableEntry<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.entry
    }
}

impl<T> DerefMut for TestableEntry<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.entry
    }
}

impl<T> From<T> for TestableEntry<T> {
    fn from(entry: T) -> TestableEntry<T> {
        TestableEntry {
            entry,
            hooks: Vec::new(),
            build_hooks: Vec::new(),
        }
    }
}

impl<T> PartialEq for TestableEntry<T>
where
    T: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.entry == other.entry
    }
}

impl<T> Eq for TestableEntry<T> where T: Eq {}

impl<T> Debug for TestableEntry<T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "TestableEntry with {:?} and {} attached hooks",
            self.entry,
            self.hooks.len()
        )
    }
}

impl From<TestableEntry<RawEntry>> for RawEntry {
    fn from(value: TestableEntry<RawEntry>) -> Self {
        value.entry
    }
}
