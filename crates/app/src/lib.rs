//! Flux application library: producers, sample projects, and the export
//! orchestrator, exposed for the `flux` CLI binary and integration tests.

pub mod audio_features;
pub mod commands;
pub mod control;
pub mod diagnostics;
pub mod export;
pub mod job;
pub mod live_audio;
pub mod persistence;
pub mod producer;
pub mod record;
pub mod samples;
