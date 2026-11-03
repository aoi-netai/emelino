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

fixture_test!(string_views_and_bounded_returns, "p2-strings", false);
fixture_test!(recursive_types_and_deep_release, "p3-recursive", true);

fixture_test!(
    standard_values_and_unicode_scalars,
    "p2-standard-values",
    false
);
fixture_test!(bytes_ownership_and_wide_lengths, "p3-bytes-owned", true);

fixture_test!(
    record_updates_order_and_ownership,
    "p7-record-updates",
    true
);
