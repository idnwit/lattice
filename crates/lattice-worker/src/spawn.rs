//! `[[spawn]]` templates from the app's manifest (06-apps.md, Runner rule 5; Manifest).
//!
//! The manifest fixes argv; the runner's `Spawn` effect picks only the cwd and the PTY size.
//! The apps runner adds the journal and the env. Expansion reads the environment of
//! `latticed` (`$SHELL`, `$HOME`), never the runner's.

use lattice_package::{SHELL_TOKEN, SpawnTemplate};

/// Default PTY size of a new process, before any resize (columns, rows).
pub(crate) const DEFAULT_SIZE: (u16, u16) = (80, 24);

/// Shell used when `$SHELL` is unset or empty.
const FALLBACK_SHELL: &str = "/bin/sh";

/// Env vars of every spawn: the terminal the `terminal` emulator implements (04-worker.md,
/// Child env rule 3; proposed). `latticed-proc` puts them over its own env.
pub(crate) const TERMINAL_ENV: [(&str, &str); 2] =
    [("TERM", "xterm-256color"), ("COLORTERM", "truecolor")];

/// argv and cwd of a template, expanded on this worker. `cwd` empty means `$HOME`, then `/`.
pub(crate) fn resolve(template: &SpawnTemplate, cwd: &str) -> (Vec<Vec<u8>>, Vec<u8>) {
    let argv = template
        .argv
        .iter()
        .map(|arg| {
            if arg == SHELL_TOKEN {
                env_or("SHELL", FALLBACK_SHELL).into_bytes()
            } else {
                arg.clone().into_bytes()
            }
        })
        .collect();
    let cwd = if cwd.is_empty() {
        env_or("HOME", "/")
    } else {
        cwd.to_owned()
    };
    (argv, cwd.into_bytes())
}

fn env_or(var: &str, fallback: &str) -> String {
    std::env::var(var)
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| fallback.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shell_token_expands_and_an_explicit_cwd_is_kept() {
        let t = SpawnTemplate {
            name: "shell".to_owned(),
            argv: vec![SHELL_TOKEN.to_owned(), "-l".to_owned()],
        };
        let (argv, cwd) = resolve(&t, "/tmp");
        assert_eq!(cwd, b"/tmp");
        assert_eq!(argv.get(1).map(Vec::as_slice), Some(&b"-l"[..]));
        assert!(
            argv.first()
                .is_some_and(|a| !a.is_empty() && a != SHELL_TOKEN.as_bytes())
        );
        let (_, home) = resolve(&t, "");
        assert!(!home.is_empty());
    }
}
