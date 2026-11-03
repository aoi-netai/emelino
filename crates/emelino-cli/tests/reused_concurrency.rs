//! Fixed source/C observations through the upstream frontend and saved input.
#[path = "compat/reused/support.rs"]
mod support;
use support::run_fixture;

macro_rules! fixture_test {
    ($test:ident, $fixture:literal, $heap:literal) => {
        #[test]
        fn $test() {
            run_fixture($fixture, $heap);
        }
    };
}

fixture_test!(
    suspended_task_resume_and_stale_completion,
    "p5-suspend",
    true
);
fixture_test!(scope_fiber_deferred_and_cause, "p6-concurrency", true);
fixture_test!(
    nested_scope_cancellation_and_reclamation,
    "p6-nested-cancel",
    true
);
