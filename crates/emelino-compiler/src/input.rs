//! Versioned, self-contained input for the reused embedded backend.
pub(crate) mod codec;
mod validate;

use crate::{Error, ErrorKind, UPSTREAM_REVISION, ir::Program};
use serde::{Deserialize, Serialize};

pub const FORMAT: &str = "emelino.backend-input";
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone)]
pub struct BackendInput {
    envelope: Envelope,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    format: String,
    schema_version: u32,
    producer_revision: String,
    stage: String,
    required_features: Vec<String>,
    program: Program,
}

fn invalid(message: impl Into<String>) -> Vec<Error> {
    vec![Error {
        kind: ErrorKind::InvalidFlowIr,
        message: format!("backend input: {}", message.into()),
    }]
}

impl BackendInput {
    /// Validate a raw, owned Flow module and discard caller-supplied derived metadata.
    pub fn new(mut program: Program) -> Result<Self, Vec<Error>> {
        validate::prepare(&mut program)?;
        Ok(Self {
            envelope: Envelope {
                format: FORMAT.into(),
                schema_version: SCHEMA_VERSION,
                producer_revision: UPSTREAM_REVISION.into(),
                stage: "flow".into(),
                required_features: Vec::new(),
                program,
            },
        })
    }

    pub fn from_json(json: &str) -> Result<Self, Vec<Error>> {
        let mut input: Envelope =
            serde_json::from_str(json).map_err(|error| invalid(error.to_string()))?;
        if input.format != FORMAT || input.schema_version != SCHEMA_VERSION {
            return Err(invalid("unsupported format or schema version"));
        }
        if input.producer_revision != UPSTREAM_REVISION {
            return Err(invalid("unsupported upstream producer revision"));
        }
        if input.stage != "flow" || !input.required_features.is_empty() {
            return Err(invalid("unsupported input stage or required features"));
        }
        validate::prepare(&mut input.program)?;
        Ok(Self { envelope: input })
    }

    pub fn to_json(&self) -> Result<String, Vec<Error>> {
        serde_json::to_string_pretty(&self.envelope)
            .map(|json| json + "\n")
            .map_err(|error| invalid(error.to_string()))
    }

    pub fn program(&self) -> &Program {
        &self.envelope.program
    }
}
