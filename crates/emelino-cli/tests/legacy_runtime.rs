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
