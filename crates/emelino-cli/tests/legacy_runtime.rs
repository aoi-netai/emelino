//! Fixed original source and C observations, with their original resource budgets.
#[path = "compat/reused/support.rs"]
mod support;

#[test]
fn legacy_cause_attempt_keeps_the_embedded_policy_of_stopping_immediately_on_defect() {
    support::run_fixture(
        "legacy-cause-attempt_keeps_the_embedded_policy_of_stopping_immediately_on_defect",
        false,
    );
}

#[test]
fn legacy_cause_cause_to_string_uses_error_show_and_caller_output_buffer() {
    support::run_fixture(
        "legacy-cause-cause_to_string_uses_error_show_and_caller_output_buffer",
        true,
    );
}

#[test]
fn legacy_cause_attempt_captures_sync_failure_and_returns_owned_cause() {
    support::run_fixture(
        "legacy-cause-attempt_captures_sync_failure_and_returns_owned_cause",
        true,
    );
}

#[test]
fn legacy_cause_cause_kind_reads_failure_without_consuming_the_cause() {
    support::run_fixture(
        "legacy-cause-cause_kind_reads_failure_without_consuming_the_cause",
        true,
    );
}

#[test]
fn legacy_cause_attempt_owns_managed_success_and_releases_discarded_results() {
    support::run_fixture(
        "legacy-cause-attempt_owns_managed_success_and_releases_discarded_results",
        true,
    );
}

#[test]
fn legacy_cause_cause_raise_preserves_the_error_and_runs_native_cleanup_before_escape() {
    support::run_fixture(
        "legacy-cause-cause_raise_preserves_the_error_and_runs_native_cleanup_before_escape",
        true,
    );
}

#[test]
fn legacy_aggregate_heapless_callbacks_accept_and_return_aggregates_containing_function_values() {
    support::run_fixture(
        "legacy-aggregate-heapless_callbacks_accept_and_return_aggregates_containing_function_values",
        false,
    );
}

#[test]
fn legacy_aggregate_failing_indirect_aggregate_calls_release_discarded_successes_and_preserve_string_errors()
 {
    support::run_fixture(
        "legacy-aggregate-failing_indirect_aggregate_calls_release_discarded_successes_and_preserve_string_errors",
        true,
    );
}
