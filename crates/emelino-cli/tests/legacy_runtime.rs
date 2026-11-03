//! Fixed original source and C observations, with their original resource budgets.
#[path = "compat/reused/support.rs"]
mod support;

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
