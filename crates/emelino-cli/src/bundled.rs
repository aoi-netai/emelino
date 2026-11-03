//! Ordinary compatibility modules derived from the pinned upstream standard sources.
//! Only Emelino.Bytes owns an additional implementation in this repository.

use std::borrow::Cow;

const EMBEDDED: &[(&str, &str)] = &[
    ("embedded/peripheral.emel", "Std.Embedded.Peripheral"),
    ("embedded/gpio.emel", "Std.Embedded.Gpio"),
    ("embedded/uart.emel", "Std.Embedded.Uart"),
    ("embedded/i2c.emel", "Std.Embedded.I2c"),
    ("embedded/spi.emel", "Std.Embedded.Spi"),
    ("embedded/board.emel", "Std.Embedded.Board"),
];

pub(crate) fn sources() -> Result<Vec<(&'static str, Cow<'static, str>)>, String> {
    let mut sources = vec![(
        "emelino/bytes.emel",
        Cow::Borrowed(include_str!("../lib/emelino/bytes.emel")),
    )];
    for &(path, module) in EMBEDDED {
        let (_, source) = emela_core::core_sources()
            .iter()
            .find(|(name, _)| *name == module)
            .ok_or_else(|| format!("pinned Emela standard source is missing: {module}"))?;
        sources.push((path, compatibility_imports(source)));
    }
    Ok(sources)
}

/// Derive only the legacy namespace's import lines. The upstream module and all
/// user source bodies remain unchanged and receive their normal frontend checks.
fn compatibility_imports(source: &'static str) -> Cow<'static, str> {
    const PREFIX: &str = "import Std.Embedded.";
    if !source.lines().any(|line| line.starts_with(PREFIX)) {
        return Cow::Borrowed(source);
    }
    let mut derived = String::with_capacity(source.len());
    for line in source.split_inclusive('\n') {
        if let Some(rest) = line.strip_prefix(PREFIX) {
            derived.push_str("import Embedded.");
            derived.push_str(rest);
        } else {
            derived.push_str(line);
        }
    }
    Cow::Owned(derived)
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
            .expect("host sha256sum is required to check the pinned module contract");
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
    fn embedded_compatibility_sources_match_fixed_legacy_hashes() {
        let report: serde_json::Value = serde_json::from_str(include_str!(
            "../../../docs/reports/reuse-boundary-fixtures.json"
        ))
        .unwrap();
        let sources = sources().unwrap();
        assert_eq!(sources.len(), 7);
        let mut checked = 0;
        for module in report["modules"].as_array().unwrap() {
            let Some(path) = module["compatibility_path"].as_str() else {
                continue;
            };
            let (_, source) = sources.iter().find(|(name, _)| *name == path).unwrap();
            assert_eq!(sha256(source), module["sha256"], "derived {path}");
            let original = emela_core::core_sources()
                .iter()
                .find(|(name, _)| *name == module["upstream_module"].as_str().unwrap())
                .unwrap()
                .1;
            assert_eq!(sha256(original), module["source_sha256"], "pinned {path}");
            checked += 1;
        }
        assert_eq!(checked, 6);
    }

    #[test]
    fn compatibility_derivation_changes_only_embedded_import_lines() {
        let source = "# import Std.Embedded.Gpio is a comment\nimport Std.Clock.{Clock}\nimport Std.Embedded.Gpio.{Gpio}\npub fn text()->String{\"import Std.Embedded.Gpio\"}\n";
        assert_eq!(
            compatibility_imports(source),
            "# import Std.Embedded.Gpio is a comment\nimport Std.Clock.{Clock}\nimport Embedded.Gpio.{Gpio}\npub fn text()->String{\"import Std.Embedded.Gpio\"}\n"
        );
    }
}
