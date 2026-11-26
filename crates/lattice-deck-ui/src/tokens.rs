//! Design tokens, generated at build time from
//! `docs/design-system/tokens.json` (the only source). Never hard-code a
//! value that lives here.

/// A type style: family stack, size, line height, weight, tracking.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TypeStyle {
    /// Font family stack (CSS list).
    pub family: &'static str,
    /// Font size, points.
    pub size: f32,
    /// Line height, points.
    pub line: f32,
    /// Font weight (100–900).
    pub weight: u16,
    /// Letter spacing, in ems.
    pub tracking_em: f32,
    /// Uppercase transform.
    pub upper: bool,
}

include!(concat!(env!("OUT_DIR"), "/tokens.rs"));
