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
fn legacy_cleanup_child_failure_closes_scope_and_waits_for_sibling_release_before_escape() {
    support::run_fixture(
        "legacy-cleanup-child_failure_closes_scope_and_waits_for_sibling_release_before_escape",
        true,
    );
}

#[test]
fn legacy_cleanup_delayed_failure_after_resume_formats_owned_error_after_the_call_frame_returns() {
    support::run_fixture(
        "legacy-cleanup-delayed_failure_after_resume_formats_owned_error_after_the_call_frame_returns",
        false,
    );
}

#[test]
fn legacy_cleanup_failing_and_infallible_scopes_share_the_fork_operation_abi() {
    support::run_fixture(
        "legacy-cleanup-failing_and_infallible_scopes_share_the_fork_operation_abi",
        true,
    );
}

#[test]
fn legacy_cleanup_failing_suspend_closure_keeps_error_through_suspending_release() {
    support::run_fixture(
        "legacy-cleanup-failing_suspend_closure_keeps_error_through_suspending_release",
        true,
    );
}

#[test]
fn legacy_cleanup_operation_failure_after_resume_stops_without_writing_a_stale_error_pointer() {
    support::run_fixture(
        "legacy-cleanup-operation_failure_after_resume_stops_without_writing_a_stale_error_pointer",
        false,
    );
}

#[test]
fn legacy_cleanup_suspend_failures_cross_calls_and_preserve_cleanup_before_escape() {
    support::run_fixture(
        "legacy-cleanup-suspend_failures_cross_calls_and_preserve_cleanup_before_escape",
        true,
    );
}

#[test]
fn legacy_cleanup_suspend_operation_failure_before_resume_reaches_its_call_edge() {
    support::run_fixture(
        "legacy-cleanup-suspend_operation_failure_before_resume_reaches_its_call_edge",
        true,
    );
}

#[test]
fn legacy_cleanup_uncaught_suspend_failure_stops_at_public_boundary_after_release() {
    support::run_fixture(
        "legacy-cleanup-uncaught_suspend_failure_stops_at_public_boundary_after_release",
        false,
    );
}

#[test]
fn legacy_cleanup_await_of_interrupted_children_returns_owned_failures_repeatedly_and_for_unit() {
    support::run_fixture(
        "legacy-cleanup-await_of_interrupted_children_returns_owned_failures_repeatedly_and_for_unit",
        true,
    );
}

#[test]
fn legacy_cleanup_await_waiters_receive_failure_after_interrupt_cleanup_before_interrupt_waiters() {
    support::run_fixture(
        "legacy-cleanup-await_waiters_receive_failure_after_interrupt_cleanup_before_interrupt_waiters",
        true,
    );
}

#[test]
fn legacy_cleanup_interrupt_cancels_unstarted_children_and_does_not_yield_for_ended_children() {
    support::run_fixture(
        "legacy-cleanup-interrupt_cancels_unstarted_children_and_does_not_yield_for_ended_children",
        true,
    );
}

#[test]
fn legacy_cleanup_interrupt_waits_for_suspending_cleanup_and_ignores_old_task_notifications() {
    support::run_fixture(
        "legacy-cleanup-interrupt_waits_for_suspending_cleanup_and_ignores_old_task_notifications",
        true,
    );
}

#[test]
fn legacy_cleanup_interruption_causes_survive_scope_and_heap_reuse_and_can_be_released_by_the_caller()
 {
    support::run_fixture(
        "legacy-cleanup-interruption_causes_survive_scope_and_heap_reuse_and_can_be_released_by_the_caller",
        true,
    );
}

#[test]
fn legacy_cleanup_join_of_an_interrupted_child_is_the_defined_panic_defect() {
    support::run_fixture(
        "legacy-cleanup-join_of_an_interrupted_child_is_the_defined_panic_defect",
        false,
    );
}

#[test]
fn legacy_cleanup_leaked_interrupt_callback_reports_ended_scope_after_slot_reuse() {
    support::run_fixture(
        "legacy-cleanup-leaked_interrupt_callback_reports_ended_scope_after_slot_reuse",
        false,
    );
}

#[test]
fn legacy_cleanup_multiple_interrupt_waiters_resume_fifo_after_release_finishes() {
    support::run_fixture(
        "legacy-cleanup-multiple_interrupt_waiters_resume_fifo_after_release_finishes",
        true,
    );
}

#[test]
fn legacy_cleanup_operation_tails_do_not_keep_a_copy_of_an_expired_cleanup_mask() {
    support::run_fixture(
        "legacy-cleanup-operation_tails_do_not_keep_a_copy_of_an_expired_cleanup_mask",
        true,
    );
}

#[test]
fn legacy_cleanup_list_error_payloads_survive_suspending_cleanup_and_are_dropped_on_cancellation() {
    support::run_fixture(
        "legacy-cleanup-list_error_payloads_survive_suspending_cleanup_and_are_dropped_on_cancellation",
        true,
    );
}

#[test]
fn legacy_cleanup_native_failure_channels_own_formatted_parse_errors_and_list_payloads() {
    support::run_fixture(
        "legacy-cleanup-native_failure_channels_own_formatted_parse_errors_and_list_payloads",
        true,
    );
}

#[test]
fn legacy_cleanup_operation_failure_before_resume_transfers_and_releases_its_string_payload() {
    support::run_fixture(
        "legacy-cleanup-operation_failure_before_resume_transfers_and_releases_its_string_payload",
        true,
    );
}

#[test]
fn legacy_cleanup_a_root_waits_for_cleanup_of_its_resumed_operation_tail() {
    support::run_fixture(
        "legacy-cleanup-a_root_waits_for_cleanup_of_its_resumed_operation_tail",
        true,
    );
}

#[test]
fn legacy_cleanup_cancellation_preserves_a_suspending_release_and_its_pending_task() {
    support::run_fixture(
        "legacy-cleanup-cancellation_preserves_a_suspending_release_and_its_pending_task",
        true,
    );
}

#[test]
fn legacy_cleanup_defects_in_suspend_protect_keep_the_configured_immediate_stop_policy() {
    support::run_fixture(
        "legacy-cleanup-defects_in_suspend_protect_keep_the_configured_immediate_stop_policy",
        false,
    );
}

#[test]
fn legacy_cleanup_scope_cancellation_runs_nested_releases_before_the_parent_continues() {
    support::run_fixture(
        "legacy-cleanup-scope_cancellation_runs_nested_releases_before_the_parent_continues",
        true,
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
fn legacy_cause_raised_interruption_crosses_sync_calls_and_attempt_without_becoming_a_value() {
    support::run_fixture(
        "legacy-cause-raised_interruption_crosses_sync_calls_and_attempt_without_becoming_a_value",
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
fn legacy_cause_attempt_waits_for_async_cleanup_before_catching_failure() {
    support::run_fixture(
        "legacy-cause-attempt_waits_for_async_cleanup_before_catching_failure",
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
fn legacy_aggregate_deferred_complete_poll_and_await_share_owned_string_aggregates_past_scope_end()
{
    support::run_fixture(
        "legacy-aggregate-deferred_complete_poll_and_await_share_owned_string_aggregates_past_scope_end",
        true,
    );
}

#[test]
fn legacy_aggregate_native_closure_captures_string_tuple_enum_and_list_after_factory_stack_returns()
{
    support::run_fixture(
        "legacy-aggregate-native_closure_captures_string_tuple_enum_and_list_after_factory_stack_returns",
        true,
    );
}

#[test]
fn legacy_aggregate_string_aggregates_own_borrowed_inputs_and_native_results_across_suspend_and_cancellation()
 {
    support::run_fixture(
        "legacy-aggregate-string_aggregates_own_borrowed_inputs_and_native_results_across_suspend_and_cancellation",
        true,
    );
}

#[test]
fn legacy_aggregate_suspended_child_captures_and_scope_results_keep_string_aggregates_until_last_release()
 {
    support::run_fixture(
        "legacy-aggregate-suspended_child_captures_and_scope_results_keep_string_aggregates_until_last_release",
        true,
    );
}

#[test]
fn legacy_aggregate_both_returns_nested_string_aggregates_and_cancels_pending_children_without_leaks()
 {
    support::run_fixture(
        "legacy-aggregate-both_returns_nested_string_aggregates_and_cancels_pending_children_without_leaks",
        true,
    );
}

#[test]
fn legacy_aggregate_scope_returns_string_enum_exits_after_success_or_interruption_and_frame_reuse()
{
    support::run_fixture(
        "legacy-aggregate-scope_returns_string_enum_exits_after_success_or_interruption_and_frame_reuse",
        true,
    );
}

#[test]
fn legacy_aggregate_handler_resume_accepts_a_string_enum_and_reclaims_it_when_cancelled_in_either_wait()
 {
    support::run_fixture(
        "legacy-aggregate-handler_resume_accepts_a_string_enum_and_reclaims_it_when_cancelled_in_either_wait",
        true,
    );
}

#[test]
fn legacy_aggregate_both_propagates_managed_failure_from_aggregate_callbacks_and_reclaims_its_sibling()
 {
    support::run_fixture(
        "legacy-aggregate-both_propagates_managed_failure_from_aggregate_callbacks_and_reclaims_its_sibling",
        true,
    );
}

#[test]
fn legacy_aggregate_native_aggregate_callbacks_own_results_after_captures_and_arguments_are_released()
 {
    support::run_fixture(
        "legacy-aggregate-native_aggregate_callbacks_own_results_after_captures_and_arguments_are_released",
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

#[test]
fn legacy_aggregate_native_string_enum_producers_use_owned_backing_and_heapless_borrowed_tuples_stay_borrowed_borrowed()
 {
    support::run_fixture(
        "legacy-aggregate-native_string_enum_producers_use_owned_backing_and_heapless_borrowed_tuples_stay_borrowed_borrowed",
        false,
    );
}

#[test]
fn legacy_aggregate_native_string_enum_producers_use_owned_backing_and_heapless_borrowed_tuples_stay_borrowed_owned()
 {
    support::run_fixture(
        "legacy-aggregate-native_string_enum_producers_use_owned_backing_and_heapless_borrowed_tuples_stay_borrowed_owned",
        true,
    );
}

#[test]
fn legacy_concurrency_scope_only_omits_deferred_pool() {
    support::run_fixture("legacy-concurrency-scope_only_omits_deferred_pool", false);
}
