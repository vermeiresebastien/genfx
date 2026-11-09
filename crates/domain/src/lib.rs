//! Flux domain: exact rational time, project document, typed commands,
//! deterministic undo/redo, and parameter evaluation.
//!
//! No GPU, OS, or media dependencies live here. Everything above this crate
//! consumes immutable descriptions and produces results.

pub mod commands;
pub mod document;
pub mod error;
pub mod params;
pub mod time;

pub use commands::{Command, Dispatcher, Transaction};
pub use document::*;
pub use error::{ErrorCode, FluxError, FluxResult, Severity};
pub use time::{FrameRate, RationalTime, TimeRange};
