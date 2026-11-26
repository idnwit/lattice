# Bundled fonts

The deck registers these at start (`lattice_deck_ui::fonts::register`), so the `mono` stack in
`docs/design-system/tokens.json` renders the same on every machine, installed fonts or not.

| File | Family | Source | License |
|---|---|---|---|
| `JetBrainsMono-Regular.woff2`, `-Medium`, `-Bold`, `-Italic` | JetBrains Mono (400, 500, 700, 400 italic) | JetBrains/JetBrainsMono v2.304, `fonts/webfonts/` | SIL OFL 1.1 (`OFL-JetBrainsMono.txt`) |
| `SymbolsNerdFontMono-Regular.ttf` | Symbols Nerd Font Mono | ryanoasis/nerd-fonts v3.4.0, `NerdFontsSymbolsOnly.zip` | MIT (`LICENSE-NerdFonts.txt`); icon sets inside keep their upstream licenses |

Symbols Nerd Font Mono is the fallback for the Private Use Area glyphs shell prompts draw
(powerline, devicons, …), which no system font has.

Licenses are listed for everyone in the root [README.md](../../../README.md#third-party-assets-and-licenses).
