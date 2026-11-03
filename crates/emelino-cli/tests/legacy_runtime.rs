//! Fixed original source and C observations, with their original resource budgets.
#[path = "compat/reused/support.rs"]
mod support;

#[test]
fn legacy_concurrency_await_preserves_string_and_list_results_past_frame_and_scope_reuse() {
    support::run_fixture(
        "legacy-concurrency-await_preserves_string_and_list_results_past_frame_and_scope_reuse",
        true,
    );
}

#[test]
fn legacy_concurrency_await_resumes_all_waiters_in_fifo_order_after_completer_finishes() {
    support::run_fixture(
        "legacy-concurrency-await_resumes_all_waiters_in_fifo_order_after_completer_finishes",
        true,
    );
}

#[test]
fn legacy_concurrency_complete_copies_borrowed_string_and_poll_retains_it_past_scope_end() {
    support::run_fixture(
        "legacy-concurrency-complete_copies_borrowed_string_and_poll_retains_it_past_scope_end",
        true,
    );
}

#[test]
fn legacy_concurrency_complete_stores_once_and_scope_end_breaks_cycles_through_captured_deferreds()
{
    support::run_fixture(
        "legacy-concurrency-complete_stores_once_and_scope_end_breaks_cycles_through_captured_deferreds",
        true,
    );
}

#[test]
fn legacy_concurrency_deferred_constructor_reuses_slots_and_scope_only_programs_omit_the_pool() {
    support::run_fixture(
        "legacy-concurrency-deferred_constructor_reuses_slots_and_scope_only_programs_omit_the_pool",
        true,
    );
}

#[test]
fn legacy_concurrency_deferred_pool_exhaustion_uses_the_defined_defect() {
    support::run_fixture(
        "legacy-concurrency-deferred_pool_exhaustion_uses_the_defined_defect",
        false,
    );
}

#[test]
fn legacy_concurrency_deferred_round_trips_int64_and_float_bits_through_await_and_poll() {
    support::run_fixture(
        "legacy-concurrency-deferred_round_trips_int64_and_float_bits_through_await_and_poll",
        true,
    );
}

#[test]
fn legacy_concurrency_deferred_transfers_tuple_values_and_boolean_flags() {
    support::run_fixture(
        "legacy-concurrency-deferred_transfers_tuple_values_and_boolean_flags",
        true,
    );
}

#[test]
fn legacy_concurrency_deferred_wait_is_a_real_resume_boundary_and_root_end_reclaims_pending_operation()
 {
    support::run_fixture(
        "legacy-concurrency-deferred_wait_is_a_real_resume_boundary_and_root_end_reclaims_pending_operation",
        true,
    );
}

#[test]
fn legacy_concurrency_ended_deferred_poll_and_await_reject_leaked_callbacks_after_slot_reuse_poll()
{
    support::run_fixture(
        "legacy-concurrency-ended_deferred_poll_and_await_reject_leaked_callbacks_after_slot_reuse_poll",
        false,
    );
}

#[test]
fn legacy_concurrency_ended_deferred_poll_and_await_reject_leaked_callbacks_after_slot_reuse_await()
{
    support::run_fixture(
        "legacy-concurrency-ended_deferred_poll_and_await_reject_leaked_callbacks_after_slot_reuse_await",
        false,
    );
}

#[test]
fn legacy_concurrency_leaked_complete_callback_checks_scope_before_recycled_slot() {
    support::run_fixture(
        "legacy-concurrency-leaked_complete_callback_checks_scope_before_recycled_slot",
        false,
    );
}

#[test]
fn legacy_concurrency_nested_scope_cancellation_unlinks_middle_tail_and_head_waiters_and_ready_deliveries()
 {
    support::run_fixture(
        "legacy-concurrency-nested_scope_cancellation_unlinks_middle_tail_and_head_waiters_and_ready_deliveries",
        true,
    );
}

#[test]
fn legacy_concurrency_poll_observes_empty_and_first_value_for_scalar_and_unit_without_yielding() {
    support::run_fixture(
        "legacy-concurrency-poll_observes_empty_and_first_value_for_scalar_and_unit_without_yielding",
        true,
    );
}

#[test]
fn legacy_concurrency_a_direct_suspend_call_can_return_an_inline_exit_into_its_caller_frame() {
    support::run_fixture(
        "legacy-concurrency-a_direct_suspend_call_can_return_an_inline_exit_into_its_caller_frame",
        true,
    );
}

#[test]
fn legacy_concurrency_await_and_join_share_fifo_waiters_and_transfer_each_result() {
    support::run_fixture(
        "legacy-concurrency-await_and_join_share_fifo_waiters_and_transfer_each_result",
        true,
    );
}

#[test]
fn legacy_concurrency_await_keeps_owned_list_results_after_the_scope_ends() {
    support::run_fixture(
        "legacy-concurrency-await_keeps_owned_list_results_after_the_scope_ends",
        true,
    );
}

#[test]
fn legacy_concurrency_await_preserves_int64_float_bits_and_bool_payloads() {
    support::run_fixture(
        "legacy-concurrency-await_preserves_int64_float_bits_and_bool_payloads",
        true,
    );
}

#[test]
fn legacy_concurrency_await_returns_success_and_can_mix_with_repeated_join_without_yielding() {
    support::run_fixture(
        "legacy-concurrency-await_returns_success_and_can_mix_with_repeated_join_without_yielding",
        true,
    );
}

#[test]
fn legacy_concurrency_await_with_a_string_exit_keeps_owned_backing_after_scope_and_frame_reuse() {
    support::run_fixture(
        "legacy-concurrency-await_with_a_string_exit_keeps_owned_backing_after_scope_and_frame_reuse",
        true,
    );
}

#[test]
fn legacy_concurrency_child_wait_is_a_resume_boundary_and_scope_end_reclaims_the_operation_tail_await()
 {
    support::run_fixture(
        "legacy-concurrency-child_wait_is_a_resume_boundary_and_scope_end_reclaims_the_operation_tail_await",
        true,
    );
}

#[test]
fn legacy_concurrency_child_wait_is_a_resume_boundary_and_scope_end_reclaims_the_operation_tail_join()
 {
    support::run_fixture(
        "legacy-concurrency-child_wait_is_a_resume_boundary_and_scope_end_reclaims_the_operation_tail_join",
        true,
    );
}

#[test]
fn legacy_concurrency_closing_a_scope_drops_ready_join_and_await_frames_before_result_delivery() {
    support::run_fixture(
        "legacy-concurrency-closing_a_scope_drops_ready_join_and_await_frames_before_result_delivery",
        true,
    );
}

#[test]
fn legacy_concurrency_leaked_await_callback_checks_the_ended_scope_before_using_the_child_result() {
    support::run_fixture(
        "legacy-concurrency-leaked_await_callback_checks_the_ended_scope_before_using_the_child_result",
        false,
    );
}

#[test]
fn legacy_concurrency_returning_await_exit_through_scope_owns_the_exit_after_scope_end() {
    support::run_fixture(
        "legacy-concurrency-returning_await_exit_through_scope_owns_the_exit_after_scope_end",
        true,
    );
}

#[test]
fn legacy_concurrency_scope_end_breaks_cycles_when_a_child_result_closure_captures_its_own_handle()
{
    support::run_fixture(
        "legacy-concurrency-scope_end_breaks_cycles_when_a_child_result_closure_captures_its_own_handle",
        true,
    );
}

#[test]
fn legacy_concurrency_closing_nested_scopes_unlinks_join_waiters_before_the_target_completes() {
    support::run_fixture(
        "legacy-concurrency-closing_nested_scopes_unlinks_join_waiters_before_the_target_completes",
        true,
    );
}

#[test]
fn legacy_concurrency_join_preserves_int64_float_bits_and_bool() {
    support::run_fixture(
        "legacy-concurrency-join_preserves_int64_float_bits_and_bool",
        true,
    );
}

#[test]
fn legacy_concurrency_join_results_own_strings_and_lists_after_scope_and_frame_reuse() {
    support::run_fixture(
        "legacy-concurrency-join_results_own_strings_and_lists_after_scope_and_frame_reuse",
        true,
    );
}

#[test]
fn legacy_concurrency_join_reuses_completed_results_without_yielding_and_waits_for_discarded_unit()
{
    support::run_fixture(
        "legacy-concurrency-join_reuses_completed_results_without_yielding_and_waits_for_discarded_unit",
        true,
    );
}

#[test]
fn legacy_concurrency_join_wakes_multiple_waiters_in_fifo_after_the_childs_last_breath() {
    support::run_fixture(
        "legacy-concurrency-join_wakes_multiple_waiters_in_fifo_after_the_childs_last_breath",
        true,
    );
}

#[test]
fn legacy_concurrency_leaked_join_callback_rejects_an_ended_scope_after_reusing_execution_slots() {
    support::run_fixture(
        "legacy-concurrency-leaked_join_callback_rejects_an_ended_scope_after_reusing_execution_slots",
        false,
    );
}

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
fn legacy_scope_scope_starts_children_at_real_suspension_and_reclaims_waiting_tasks() {
    support::run_fixture(
        "legacy-scope-scope_starts_children_at_real_suspension_and_reclaims_waiting_tasks",
        false,
    );
}

#[test]
fn legacy_scope_scope_cancels_unstarted_children_before_running_their_function() {
    support::run_fixture(
        "legacy-scope-scope_cancels_unstarted_children_before_running_their_function",
        false,
    );
}

#[test]
fn legacy_scope_completed_children_and_queued_events_do_not_leak_across_scope_reuse() {
    support::run_fixture(
        "legacy-scope-completed_children_and_queued_events_do_not_leak_across_scope_reuse",
        false,
    );
}

#[test]
fn legacy_scope_closing_outer_scope_reclaims_nested_scopes_and_their_tasks() {
    support::run_fixture(
        "legacy-scope-closing_outer_scope_reclaims_nested_scopes_and_their_tasks",
        false,
    );
}

#[test]
fn legacy_scope_children_can_fork_siblings_and_inherit_the_scope_call_evidence() {
    support::run_fixture(
        "legacy-scope-children_can_fork_siblings_and_inherit_the_scope_call_evidence",
        false,
    );
}

#[test]
fn legacy_scope_fork_exhaustion_is_a_fixed_pool_defect_before_children_start() {
    support::run_fixture(
        "legacy-scope-fork_exhaustion_is_a_fixed_pool_defect_before_children_start",
        false,
    );
}

#[test]
fn legacy_scope_scope_keeps_managed_captures_and_boxes_reference_results_without_leaks() {
    support::run_fixture(
        "legacy-scope-scope_keeps_managed_captures_and_boxes_reference_results_without_leaks",
        false,
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

#[test]
fn legacy_concurrency_scope_only_omits_deferred_pool() {
    support::run_fixture("legacy-concurrency-scope_only_omits_deferred_pool", false);
}
