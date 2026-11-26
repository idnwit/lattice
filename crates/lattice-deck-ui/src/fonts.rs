//! Fonts the design system names, shipped inside the app (`fonts/README.md`).
//!
//! The `mono` stack (`tokens.json`) is `JetBrains Mono`, then `Symbols Nerd Font Mono` for the
//! Private Use Area glyphs shell prompts draw, then the platform's monospace. Neither font is
//! on a stock macOS or iOS, so the app registers both before its first view: call
//! [`register`] once, at the top of the root component.

use nsh_core::prelude::*;

/// Every bundled font file: JetBrains Mono 400, 500, 700 (bold runs), 400 italic, and
/// Symbols Nerd Font Mono.
const FILES: [&[u8]; 5] = [
    include_bytes!("../fonts/JetBrainsMono-Regular.woff2"),
    include_bytes!("../fonts/JetBrainsMono-Medium.woff2"),
    include_bytes!("../fonts/JetBrainsMono-Bold.woff2"),
    include_bytes!("../fonts/JetBrainsMono-Italic.woff2"),
    include_bytes!("../fonts/SymbolsNerdFontMono-Regular.ttf"),
];

/// Registers the bundled fonts. Text naming their families lays out with them from then on.
/// A font the platform refuses leaves its stack to fall back.
pub fn register() {
    for data in FILES {
        let answer = register_font(data);
        spawn(async move {
            if let Err(e) = answer.await {
                tracing::warn!(error = %e, "bundled font refused");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bundled_font_names_a_family_of_the_mono_stack() {
        let stack = crate::tokens::font::MONO;
        for data in FILES {
            let data = nsh_core::font_data(data).unwrap();
            let family = nsh_core::font_family(&data).unwrap();
            assert!(
                stack.contains(&format!("'{family}'")),
                "{family} in {stack}"
            );
        }
    }
}
