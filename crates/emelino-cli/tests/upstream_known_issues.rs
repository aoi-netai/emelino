//! UPSTREAM-001 regression: observe the pinned bug without redefining the language.
#[path = "compat/reused/support.rs"]
mod support;

#[test]
fn upstream_001_specialized_consts_keep_two_initializers_until_upstream_fix() {
    support::run_fixture("upstream-001", false);
}
