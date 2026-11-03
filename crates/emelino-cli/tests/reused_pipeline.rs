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

fixture_test!(numeric_boundaries_and_float_bits, "p1-numeric", false);

fixture_test!(bytes_ownership_and_wide_lengths, "p3-bytes-owned", true);
