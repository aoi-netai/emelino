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

fixture_test!(failure_payloads_and_escape, "p2-failures", false);
fixture_test!(closure_captures_order_and_failure, "p4-closures", true);
fixture_test!(handler_layers_labels_and_cleanup, "p4-handlers", true);
