//! Design tokens, generated at build time from
//! `docs/design-system/tokens.json` (the only source). Never hard-code a
//! value that lives here.

#![allow(
    dead_code,
    reason = "generated mirror of tokens.json: screens use a subset"
)]

/// A type style: family stack, size, line height, weight, tracking.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TypeStyle {
    /// Font family stack (CSS list).
    pub(crate) family: &'static str,
    /// Font size, points.
    pub(crate) size: f32,
    /// Line height, points.
    pub(crate) line: f32,
    /// Font weight (100–900).
    pub(crate) weight: u16,
    /// Letter spacing, in ems.
    pub(crate) tracking_em: f32,
    /// Uppercase transform.
    pub(crate) upper: bool,
}

include!(concat!(env!("OUT_DIR"), "/tokens.rs"));
