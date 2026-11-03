//! Embedded interfaces and board handlers owned by emelino.
//! Imported from Emela v0.26.1 (Apache-2.0); generic core/std stays upstream.

pub(crate) fn sources() -> &'static [(&'static str, &'static str)] {
    &[
        (
            "embedded/interrupt.emel",
            include_str!("../lib/embedded/interrupt.emel"),
        ),
        (
            "embedded/peripheral.emel",
            include_str!("../lib/embedded/peripheral.emel"),
        ),
        (
            "embedded/gpio.emel",
            include_str!("../lib/embedded/gpio.emel"),
        ),
        (
            "embedded/uart.emel",
            include_str!("../lib/embedded/uart.emel"),
        ),
        (
            "embedded/i2c.emel",
            include_str!("../lib/embedded/i2c.emel"),
        ),
        (
            "embedded/spi.emel",
            include_str!("../lib/embedded/spi.emel"),
        ),
        (
            "embedded/board.emel",
            include_str!("../lib/embedded/board.emel"),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::process::{Command, Stdio};

    fn sha256(source: &str) -> String {
        let mut child = Command::new("sha256sum")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("host sha256sum is required to check the imported module baseline");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(source.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .to_owned()
    }

    #[test]
    fn embedded_import_provenance_matches_record() {
        let report: serde_json::Value = serde_json::from_str(include_str!(
            "../../../docs/reports/reuse-boundary-fixtures.json"
        ))
        .unwrap();
        let mut checked = 0;
        for module in report["modules"].as_array().unwrap() {
            let Some(path) = module["module_path"].as_str() else {
                continue;
            };
            // These hashes describe the import, not the evolving local API.
            // Check the pinned provenance without resetting the current sources
            // or rewriting the historical record when an interface changes.
            assert!(sources().iter().any(|(name, _)| *name == path));
            let upstream_name = module["upstream_module"].as_str().unwrap();
            let (_, source) = emela_core::core_sources()
                .iter()
                .find(|(name, _)| *name == upstream_name)
                .unwrap();
            assert_eq!(sha256(source), module["source_sha256"], "upstream {path}");
            let imported = source.replace("Std.Embedded", "Embedded");
            assert_eq!(sha256(&imported), module["sha256"], "imported {path}");
            checked += 1;
        }
        assert_eq!(checked, 6);
    }

    #[test]
    fn embedded_sources_have_no_upstream_module_dependency() {
        // The real source route excludes upstream embedded modules even while
        // the pinned release still supplies them. Generic std remains intact.
        let upstream: Vec<_> = crate::frontend::upstream_sources().collect();
        assert!(
            upstream
                .iter()
                .all(|(name, _)| { *name != "Std.Embedded" && !name.starts_with("Std.Embedded.") })
        );
        for name in ["Std.Bytes", "Std.Clock", "Std.Console", "Std.Instant"] {
            assert!(upstream.iter().any(|(module, _)| *module == name));
        }
        for (name, source) in sources() {
            assert!(!source.contains("Std.Embedded"), "{name}");
            let parsed = emela_syntax::parse(source);
            assert!(parsed.diagnostics().is_empty(), "{name}");
        }
    }
}
