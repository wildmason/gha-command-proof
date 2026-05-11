use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::command::CommandRecord;
use crate::envfile::EnvFileAnalysis;

#[derive(Clone, Debug, Serialize)]
pub struct Receipt {
    pub schema_version: u8,
    pub tool: ToolReceipt,
    pub checked_at: DateTime<Utc>,
    pub mode: String,
    pub summary: Summary,
    pub checks: Vec<Check>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<CommandRecord>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub env_files: Vec<EnvFileAnalysis>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ToolReceipt {
    pub name: &'static str,
    pub version: &'static str,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Summary {
    pub passed: usize,
    pub warned: usize,
    pub failed: usize,
    pub skipped: usize,
}

impl Summary {
    pub fn from_checks(checks: &[Check]) -> Self {
        let mut summary = Self::default();
        for check in checks {
            match check.status {
                CheckStatus::Pass => summary.passed += 1,
                CheckStatus::Warn => summary.warned += 1,
                CheckStatus::Fail => summary.failed += 1,
                CheckStatus::Skip => summary.skipped += 1,
            }
        }
        summary
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Check {
    pub id: String,
    pub status: CheckStatus,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub details: BTreeMap<String, String>,
}

impl Check {
    pub fn pass(
        id: impl Into<String>,
        message: impl Into<String>,
        location: Option<Location>,
    ) -> Self {
        Self::new(id, CheckStatus::Pass, message, location)
    }

    pub fn warn(
        id: impl Into<String>,
        message: impl Into<String>,
        location: Option<Location>,
    ) -> Self {
        Self::new(id, CheckStatus::Warn, message, location)
    }

    pub fn fail(
        id: impl Into<String>,
        message: impl Into<String>,
        location: Option<Location>,
    ) -> Self {
        Self::new(id, CheckStatus::Fail, message, location)
    }

    pub fn skip(
        id: impl Into<String>,
        message: impl Into<String>,
        location: Option<Location>,
    ) -> Self {
        Self::new(id, CheckStatus::Skip, message, location)
    }

    pub fn with_detail(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.details.insert(key.into(), value.into());
        self
    }

    fn new(
        id: impl Into<String>,
        status: CheckStatus,
        message: impl Into<String>,
        location: Option<Location>,
    ) -> Self {
        Self {
            id: id.into(),
            status,
            message: message.into(),
            location,
            details: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
    Skip,
}

#[derive(Clone, Debug, Serialize)]
pub struct Location {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
}

impl Location {
    pub fn new(source: Option<String>, line: Option<usize>) -> Self {
        Self { source, line }
    }

    pub fn line(source: Option<&str>, line: usize) -> Self {
        Self {
            source: source.map(ToString::to_string),
            line: Some(line),
        }
    }
}
