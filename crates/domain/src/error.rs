//! Typed error model. Every error carries a stable code the UI can act on.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    GraphTypeMismatch,
    GraphCycle,
    GraphRequiredInput,
    ParamNonfinite,
    ShaderCompile,
    GpuDeviceLost,
    GpuLimit,
    MediaMissing,
    MediaDecode,
    AudioDeviceLost,
    AudioOverrun,
    OscMalformed,
    OscRateLimit,
    SourcePermission,
    SourceLost,
    SdkMissing,
    ExportEncoder,
    ExportSourceUnrecorded,
    DiskFull,
    ProjectCorrupt,
    ProjectVersion,
    RevisionConflict,
    NotFound,
    InvalidCommand,
    Io,
    BlockedExternal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Info,
    Warning,
    Error,
    Fatal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FluxError {
    pub code: ErrorCode,
    pub subsystem: String,
    pub severity: Severity,
    pub message: String,
    #[serde(default)]
    pub recovery: String,
    #[serde(default)]
    pub entity_ids: Vec<String>,
    #[serde(default)]
    pub detail: String,
}

impl FluxError {
    pub fn new(code: ErrorCode, subsystem: &str, message: impl Into<String>) -> Self {
        Self {
            code,
            subsystem: subsystem.into(),
            severity: Severity::Error,
            message: message.into(),
            recovery: String::new(),
            entity_ids: vec![],
            detail: String::new(),
        }
    }

    pub fn with_recovery(mut self, r: impl Into<String>) -> Self {
        self.recovery = r.into();
        self
    }

    pub fn with_ids(mut self, ids: Vec<String>) -> Self {
        self.entity_ids = ids;
        self
    }
}

impl std::fmt::Display for FluxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{:?}] {}", self.code, self.message)
    }
}

impl std::error::Error for FluxError {}

impl From<std::io::Error> for FluxError {
    fn from(e: std::io::Error) -> Self {
        let code = if e.kind() == std::io::ErrorKind::StorageFull {
            ErrorCode::DiskFull
        } else {
            ErrorCode::Io
        };
        FluxError::new(code, "io", e.to_string())
    }
}

pub type FluxResult<T> = Result<T, FluxError>;
