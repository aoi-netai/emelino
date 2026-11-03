//! Fixed source/C observations through the upstream frontend and saved input.
#[path = "compat/reused/support.rs"]
mod support;
use std::collections::BTreeSet;
use std::{fs, path::Path};
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
fixture_test!(external_scalar_bits, "p1-external", false);
fixture_test!(string_views_and_bounded_returns, "p2-strings", false);
fixture_test!(recursive_types_and_deep_release, "p3-recursive", true);

fixture_test!(
    standard_values_and_unicode_scalars,
    "p2-standard-values",
    false
);
fixture_test!(bytes_ownership_and_wide_lengths, "p3-bytes-owned", true);
fixture_test!(
    bytes_unicode_and_unsigned_traits,
    "p3-bytes-transform",
    true
);
fixture_test!(
    record_updates_order_and_ownership,
    "p7-record-updates",
    true
);

#[test]
fn coverage_ledger_retains_every_fixed_definition_context_and_test() {
    let docs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs");
    let read = |path: &str| -> serde_json::Value {
        serde_json::from_slice(&fs::read(docs.join(path)).unwrap()).unwrap()
    };
    let ledger = read("compiler-coverage.json");
    let definitions = read("reference/legacy-flow/flow-ir-inventory.json");
    let backend = read("reference/legacy-flow/flow-backend-inventory.json");
    let contexts = read("reference/legacy-flow/flow-coverage.json");
    let ids = |rows: &serde_json::Value| -> BTreeSet<String> {
        let rows = rows.as_array().unwrap();
        let ids: BTreeSet<_> = rows
            .iter()
            .map(|row| row["id"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(rows.len(), ids.len(), "duplicate IDs");
        ids
    };
    assert_eq!(ids(&ledger["records"]), ids(&definitions["items"]));
    assert_eq!(ids(&ledger["records"]), ids(&backend["records"]));
    let expected_contexts: BTreeSet<_> = contexts["items"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|item| {
            item["contexts"].as_array().unwrap().iter().map(|context| {
                (
                    item["id"].as_str().unwrap(),
                    context["context"].as_str().unwrap(),
                )
            })
        })
        .collect();
    let actual_contexts: BTreeSet<_> = ledger["contexts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| {
            (
                item["id"].as_str().unwrap(),
                item["context"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(expected_contexts, actual_contexts);
    assert_eq!(
        actual_contexts.len(),
        ledger["contexts"].as_array().unwrap().len()
    );
    let test_key = |row: &serde_json::Value| {
        (
            row["target"].as_str().unwrap().to_owned(),
            row["name"].as_str().unwrap().to_owned(),
        )
    };
    let expected_tests: BTreeSet<_> = backend["records"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|row| row["tests"].as_array().unwrap().iter().map(test_key))
        .collect();
    let actual_tests: BTreeSet<_> = ledger["tests"]
        .as_array()
        .unwrap()
        .iter()
        .map(test_key)
        .collect();
    assert_eq!(expected_tests, actual_tests);
    assert_eq!(
        actual_tests.len(),
        ledger["tests"].as_array().unwrap().len()
    );
    assert_eq!(ids(&ledger["requirements"]).len(), 10);
    for collection in ["records", "contexts", "tests", "requirements"] {
        for row in ledger[collection].as_array().unwrap() {
            let status = row["status"].as_str().unwrap();
            assert!(matches!(
                status,
                "pending"
                    | "mapped"
                    | "verified"
                    | "not_applicable"
                    | "replaced"
                    | "upstream_known_issue"
            ));
            if !matches!(status, "pending" | "mapped") {
                assert!(!row["reason"].as_str().unwrap().is_empty());
                let evidence = row["evidence"].as_array().unwrap();
                assert!(!evidence.is_empty(), "{collection}: missing final evidence");
                for reference in evidence {
                    let path = reference["path"].as_str().unwrap();
                    assert!(
                        !Path::new(path).is_absolute() && !path.split('/').any(|part| part == "..")
                    );
                    assert!(docs.join("..").join(path).is_file(), "missing {path}");
                }
            }
        }
    }
}

#[test]
fn source_coverage_retains_every_fixed_program_and_original_observation() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let read = |path: &str| -> serde_json::Value {
        serde_json::from_slice(&fs::read(root.join(path)).unwrap()).unwrap()
    };
    let ledger = read("docs/compiler-coverage.json");
    let coverage = read("docs/reports/reuse-source-coverage.json");
    let manifest = read("crates/emelino-cli/tests/compat/legacy-corpus/manifest.json");
    let programs = manifest["programs"].as_array().unwrap();
    let expected_tests: BTreeSet<_> = programs
        .iter()
        .flat_map(|program| program["legacy_tests"].as_array().unwrap())
        .map(|name| name.as_str().unwrap())
        .collect();
    let cases = coverage["cases"].as_array().unwrap();
    let actual_tests: BTreeSet<_> = cases
        .iter()
        .map(|case| case["legacy_test"].as_str().unwrap())
        .collect();
    assert_eq!(expected_tests.len(), 174);
    assert_eq!(cases.len(), 174);
    assert_eq!(actual_tests, expected_tests);
    let expected_programs: BTreeSet<_> = programs
        .iter()
        .map(|program| program["id"].as_str().unwrap())
        .collect();
    let actual_programs: BTreeSet<_> = cases
        .iter()
        .flat_map(|case| case["programs"].as_array().unwrap())
        .map(|program| program.as_str().unwrap())
        .collect();
    assert_eq!(expected_programs.len(), 169);
    assert_eq!(actual_programs, expected_programs);
    let additional = coverage["additional_nonruntime_cases"].as_array().unwrap();
    let additional_ids: BTreeSet<_> = additional
        .iter()
        .map(|case| case["legacy_test"].as_str().unwrap())
        .collect();
    assert_eq!(additional.len(), 3);
    assert_eq!(
        additional_ids,
        BTreeSet::from([
            "frontend::embedded_backend_does_not_supply_the_old_std_namespace",
            "frontend::embedded_backend_module_names_cannot_be_shadowed_by_project_files",
            "sources::embedded_sources_are_supplied_by_the_backend_without_shadowing_core",
        ])
    );
    for case in cases.iter().chain(additional) {
        let evidence = case["evidence"].as_array().unwrap();
        if case["status"] != "pending" {
            assert!(!evidence.is_empty(), "{}", case["legacy_test"]);
        }
        for reference in evidence {
            assert!(root.join(reference["report"].as_str().unwrap()).is_file());
            assert!(
                root.join(reference["test"]["path"].as_str().unwrap())
                    .is_file()
            );
        }
    }
    if ledger["completion"]["finalized"] == true {
        let source_review = read("docs/reports/reuse-source-record-review.json");
        let reviewed_records: Vec<_> = source_review["groups"]
            .as_object()
            .unwrap()
            .values()
            .flat_map(|group| group["records"].as_array().unwrap())
            .map(|id| id.as_str().unwrap())
            .collect();
        let unique_reviewed: BTreeSet<_> = reviewed_records.iter().copied().collect();
        assert_eq!(reviewed_records.len(), unique_reviewed.len());
        let source_records: BTreeSet<_> = ledger["records"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| {
                matches!(
                    row["source_scope"].as_str(),
                    Some("source" | "source_declaration")
                )
            })
            .map(|row| row["id"].as_str().unwrap())
            .collect();
        assert_eq!(source_records, unique_reviewed);
        for collection in ["records", "contexts", "tests", "requirements"] {
            for row in ledger[collection].as_array().unwrap() {
                assert!(
                    !matches!(row["status"].as_str().unwrap(), "pending" | "mapped"),
                    "unfinished {collection}: {row}"
                );
            }
        }
        for case in cases.iter().chain(additional) {
            assert!(
                !matches!(case["status"].as_str().unwrap(), "pending" | "mapped"),
                "unfinished source observation: {case}"
            );
        }
    }
}
