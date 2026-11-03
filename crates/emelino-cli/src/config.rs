//! Explicit host entries and resource limits, independent of project discovery.
use emelino_compiler::{ExternalAdapter, ExternalResult, Options};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub exports: Vec<Export>,
    pub entry: Option<String>,
    pub sources: Vec<String>,
    pub string_capacity: Option<u16>,
    pub peripheral_buffer_capacity: Option<u16>,
    pub heap_bytes: Option<u32>,
    pub const_diagnostic_bytes: Option<u16>,
    pub fiber_capacity: Option<u16>,
    pub frame_capacity: Option<u16>,
    pub frame_bytes: Option<u16>,
    pub task_capacity: Option<u16>,
    pub event_capacity: Option<u16>,
    pub deferred_capacity: Option<u16>,
    pub task_adapters: Vec<TaskAdapter>,
    pub external_adapters: Vec<SyncAdapter>,
    pub isr_entries: Vec<String>,
    pub isr_safe_externals: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Export {
    pub function: String,
    pub symbol: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskAdapter {
    pub symbol: String,
    pub adapter: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SyncAdapter {
    pub symbol: String,
    pub adapter: String,
    pub result: ResultOwnership,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultOwnership {
    Borrowed,
    Owned,
    StringBuffer,
}

impl Config {
    pub fn from_json(text: &str) -> Result<Self, String> {
        serde_json::from_str(text).map_err(|error| format!("invalid configuration: {error}"))
    }

    pub fn export_pairs(&self) -> Result<Vec<(String, String)>, String> {
        if self.exports.is_empty() {
            return Err("configuration must select at least one public function in exports".into());
        }
        unique(
            self.exports.iter().map(|export| export.function.as_str()),
            "export function",
        )?;
        unique(
            self.exports.iter().map(|export| export.symbol.as_str()),
            "export symbol",
        )?;
        Ok(self
            .exports
            .iter()
            .map(|export| (export.function.clone(), export.symbol.clone()))
            .collect())
    }

    pub fn options(&self) -> Result<Options, String> {
        unique(
            self.task_adapters
                .iter()
                .map(|adapter| adapter.symbol.as_str()),
            "task adapter",
        )?;
        unique(
            self.external_adapters
                .iter()
                .map(|adapter| adapter.symbol.as_str()),
            "external adapter",
        )?;
        unique(self.isr_entries.iter().map(String::as_str), "ISR entry")?;
        unique(
            self.isr_safe_externals.iter().map(String::as_str),
            "ISR external",
        )?;
        let mut options = Options::default().with_entry(self.entry.as_deref().unwrap_or(""));
        macro_rules! capacity {
            ($field:ident, $method:ident) => {
                if let Some(value) = self.$field {
                    options = options.$method(value);
                }
            };
        }
        capacity!(string_capacity, with_string_capacity);
        capacity!(peripheral_buffer_capacity, with_peripheral_buffer_capacity);
        capacity!(heap_bytes, with_heap_bytes);
        capacity!(const_diagnostic_bytes, with_const_diagnostic_bytes);
        capacity!(fiber_capacity, with_fiber_capacity);
        capacity!(frame_capacity, with_frame_capacity);
        capacity!(frame_bytes, with_frame_bytes);
        capacity!(task_capacity, with_task_capacity);
        capacity!(event_capacity, with_event_capacity);
        capacity!(deferred_capacity, with_deferred_capacity);
        for adapter in &self.task_adapters {
            options = options.with_task_adapter(&adapter.symbol, &adapter.adapter);
        }
        for adapter in &self.external_adapters {
            let result = match adapter.result {
                ResultOwnership::Borrowed => ExternalResult::Borrowed,
                ResultOwnership::Owned => ExternalResult::Owned,
                ResultOwnership::StringBuffer => ExternalResult::StringBuffer,
            };
            options = options.with_external_adapter(
                &adapter.symbol,
                ExternalAdapter::new(&adapter.adapter, result),
            );
        }
        for entry in &self.isr_entries {
            options = options.with_isr_entry(entry);
        }
        for symbol in &self.isr_safe_externals {
            options = options.with_isr_safe_external(symbol);
        }
        Ok(options)
    }

    /// Saved inputs already contain their selected functions and C names.
    pub fn replay(&self) -> Self {
        Self {
            exports: Vec::new(),
            sources: Vec::new(),
            ..self.clone()
        }
    }

    pub fn validate_replay(&self) -> Result<(), String> {
        if !self.exports.is_empty() || !self.sources.is_empty() {
            return Err("saved backend input already owns exports; use its program.config.json without exports or sources".into());
        }
        Ok(())
    }

    pub fn validate_entry(&self, program: &emelino_compiler::ir::Program) -> Result<(), String> {
        for (_, function) in program
            .functions
            .iter()
            .filter(|(_, function)| function.exported)
        {
            if matches!(
                program.core.ty(function.ret),
                emelino_compiler::ir::Type::Evidence
            ) || function.params.iter().any(|parameter| {
                matches!(
                    program.core.ty(function.vars[*parameter].ty),
                    emelino_compiler::ir::Type::Evidence
                )
            }) {
                return Err(format!(
                    "public C entry `{}` cannot expose effect evidence; supply a concrete public wrapper with its handlers",
                    function.name
                ));
            }
        }
        if let Some(entry) = &self.entry {
            let matches = program
                .functions
                .iter()
                .filter(|(_, function)| function.exported && function.name == *entry)
                .count();
            if entry.is_empty() || matches != 1 {
                return Err(format!(
                    "configured entry `{entry}` must select exactly one exported function"
                ));
            }
        }
        Ok(())
    }
}

fn unique<'a>(values: impl Iterator<Item = &'a str>, kind: &str) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for value in values {
        if value.is_empty() || !seen.insert(value) {
            return Err(format!("empty or duplicate {kind}: `{value}`"));
        }
    }
    Ok(())
}
