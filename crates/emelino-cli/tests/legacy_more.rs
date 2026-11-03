//! Additional fixed original source observations and existing internal C probes.
#[path = "compat/reused/support.rs"]
mod support;

#[test]
fn legacy_suspend_completed_roots_release_waiting_operation_tasks_and_ignore_their_queued_events() {
    support::run_fixture(
        "legacy-suspend-completed_roots_release_waiting_operation_tasks_and_ignore_their_queued_events",
        true,
    );
}

#[test]
fn legacy_suspend_immediate_resume_finishes_the_operation_body_before_the_caller_continues() {
    support::run_fixture(
        "legacy-suspend-immediate_resume_finishes_the_operation_body_before_the_caller_continues",
        true,
    );
}

#[test]
fn legacy_suspend_delayed_resume_runs_the_caller_until_its_next_suspension() {
    support::run_fixture(
        "legacy-suspend-delayed_resume_runs_the_caller_until_its_next_suspension",
        true,
    );
}

#[test]
fn legacy_suspend_duplicate_resume_reports_the_operation_and_handler() {
    support::run_fixture(
        "legacy-suspend-duplicate_resume_reports_the_operation_and_handler",
        false,
    );
}

#[test]
fn legacy_suspend_suspend_handler_source_waits_and_resumes_into_the_callers_result() {
    support::run_fixture(
        "legacy-suspend-suspend_handler_source_waits_and_resumes_into_the_callers_result",
        true,
    );
}

#[test]
fn legacy_closures_e805431f1035638cad96() {
    support::run_fixture("legacy-closures-e805431f1035638cad96", true);
}

#[test]
fn legacy_closures_2e02752865a6418a1e52() {
    support::run_fixture("legacy-closures-2e02752865a6418a1e52", true);
}

#[test]
fn legacy_closures_31c21eecada04937f501() {
    support::run_fixture("legacy-closures-31c21eecada04937f501", true);
}

#[test]
fn legacy_closures_9fa3708f819ff8b76dc2() {
    support::run_fixture("legacy-closures-9fa3708f819ff8b76dc2", true);
}

#[test]
fn legacy_closures_903a9b99cfdf07a02684() {
    support::run_fixture("legacy-closures-903a9b99cfdf07a02684", true);
}

#[test]
fn legacy_closures_09dfd633dcd1f95f6a18() {
    support::run_fixture("legacy-closures-09dfd633dcd1f95f6a18", true);
}

#[test]
fn legacy_closures_c587ef1e19686afdfe06() {
    support::run_fixture("legacy-closures-c587ef1e19686afdfe06", true);
}

#[test]
fn legacy_closures_f9a6b6eea98c9abe5d11() {
    support::run_fixture("legacy-closures-f9a6b6eea98c9abe5d11", true);
}

#[test]
fn legacy_closures_8e7fb878276e03451814() {
    support::run_fixture("legacy-closures-8e7fb878276e03451814", true);
}

#[test]
fn legacy_closures_3bee0ac6dc33827ec422() {
    support::run_fixture("legacy-closures-3bee0ac6dc33827ec422", true);
}

#[test]
fn legacy_recursive_4bd3bbef2dbb773478d2() {
    support::run_fixture("legacy-recursive-4bd3bbef2dbb773478d2", true);
}

#[test]
fn legacy_recursive_8f6003cb6b231900471a() {
    support::run_fixture("legacy-recursive-8f6003cb6b231900471a", true);
}

#[test]
fn legacy_recursive_3e99d1b04faf8e90bba0() {
    support::run_fixture("legacy-recursive-3e99d1b04faf8e90bba0", true);
}
