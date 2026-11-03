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

fixture_test!(monomorphic_const_order_and_defect_cache, "p2-consts", false);
