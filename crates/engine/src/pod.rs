//! Minimal derive shim so the engine crate can carry the exact 96-byte
//! `FrameUniform` layout without a `bytemuck` dependency. The render crate has
//! bytemuck and uses the same struct definition, so the layout stays in sync.
//!
//! These traits mirror `bytemuck::{Pod, Zeroable}` only in name.
pub trait Pod: Copy + 'static {}
pub trait Zeroable: Sized {}
