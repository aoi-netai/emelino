//! Additional fixed original source observations and existing internal C probes.
#[path = "compat/reused/support.rs"]
mod support;

#[test]
fn legacy_recursive_4bd3bbef2dbb773478d2() {
    support::run_fixture("legacy-recursive-4bd3bbef2dbb773478d2", true);
}

#[test]
fn legacy_recursive_8f6003cb6b231900471a() {
    support::run_fixture("legacy-recursive-8f6003cb6b231900471a", true);
}
