use super::*;
use crate::model::{Attrs, Color, Run};
use lattice_app::Effect;
use lattice_state::{Draft, ListChange, Synced, decode, encode};

fn params() -> TermParams {
    TermParams::new("/".to_owned())
}

fn shell() -> ProcRef {
    ProcRef::from_raw(1)
}

#[test]
fn start_spawns_the_shell_template() {
    let mut ctx = Ctx::new();
    let mut state = TermState::default();
    let mut t = Terminal::new(&params());
    t.start(&mut ctx, &mut state).unwrap();
    assert_eq!(
        ctx.take_effects(),
        vec![lattice_app::Effect::Spawn {
            template: "shell".to_owned(),
            cwd: "/".to_owned(),
            size: Some((80, 24)),
        }]
    );
}

fn started() -> (Terminal, TermState) {
    let mut ctx = Ctx::new();
    let mut state = TermState::default();
    let mut t = Terminal::new(&params());
    t.start(&mut ctx, &mut state).unwrap();
    (t, state)
}

#[test]
fn input_goes_to_the_shell_and_changes_no_state() {
    let (mut t, mut state) = started();
    let shell = ProcRef::from_raw(1);
    let mut ctx = Ctx::new();
    t.on_output(&mut ctx, &mut state, shell, b"$ ");
    let rev = state.rev();
    t.input(&mut ctx, &mut state, Input::new(b"ls\r".to_vec()))
        .unwrap();
    assert_eq!(
        ctx.take_effects(),
        vec![lattice_app::Effect::Write {
            proc: shell,
            bytes: b"ls\r".to_vec()
        }]
    );
    assert_eq!(state.rev(), rev);
}

#[test]
fn zero_size_is_refused() {
    let (mut t, mut state) = started();
    let got = t.resize(&mut Ctx::new(), &mut state, Resize::new(0, 24));
    assert_eq!(got, Err(TermError::BadSize));
}

#[test]
fn clear_outside_alt_screen_is_a_clean_point() {
    let (mut t, mut state) = started();
    let mut ctx = Ctx::new();
    t.on_output(
        &mut ctx,
        &mut state,
        ProcRef::from_raw(1),
        b"\x1b[2J\x1b[3J",
    );
    assert_eq!(ctx.take_effects(), vec![lattice_app::Effect::Trim]);
    t.on_output(
        &mut ctx,
        &mut state,
        ProcRef::from_raw(1),
        b"\x1b[?1049h\x1b[2J\x1b[3J",
    );
    assert!(ctx.take_effects().is_empty(), "inside the alternate screen");
}

#[test]
fn shell_exit_finishes_the_instance() {
    let (mut t, mut state) = started();
    let mut ctx = Ctx::new();
    t.on_exit(
        &mut ctx,
        &mut state,
        ProcRef::from_raw(1),
        ExitStatus::Code(0),
    );
    assert_eq!(ctx.take_effects(), vec![lattice_app::Effect::Finish]);
}

/// A runner driven as the host drives it: one commit per reducer call.
struct Host {
    t: Terminal,
    state: TermState,
    /// Follows by deltas alone, as a deck does.
    replica: TermState,
    effects: Vec<Effect>,
}

impl Host {
    fn new(params: &TermParams) -> Self {
        let mut host = Self {
            t: Terminal::new(params),
            state: TermState::default(),
            replica: TermState::default(),
            effects: Vec::new(),
        };
        host.step(|t, ctx, state| t.start(ctx, state).unwrap());
        host
    }

    fn step(&mut self, f: impl FnOnce(&mut Terminal, &mut Ctx, &mut TermState)) {
        let mut ctx = Ctx::new();
        f(&mut self.t, &mut ctx, &mut self.state);
        if self.state.has_draft() {
            let change = encode(&Draft(&self.state)).unwrap();
            self.replica.apply(decode(&change).unwrap()).unwrap();
        }
        self.state.commit();
        self.effects.extend(ctx.take_effects());
    }

    fn out(&mut self, bytes: &[u8]) {
        self.step(|t, ctx, state| t.on_output(ctx, state, shell(), bytes));
    }

    fn input(&mut self, bytes: &[u8]) -> Result<(), TermError> {
        let mut got = Ok(());
        self.step(|t, ctx, state| got = t.input(ctx, state, Input::new(bytes.to_vec())));
        got
    }

    fn resize(&mut self, cols: u16, rows: u16) -> Result<Size, TermError> {
        let mut got = Err(TermError::BadSize);
        self.step(|t, ctx, state| got = t.resize(ctx, state, Resize::new(cols, rows)));
        got
    }

    fn screen(&self) -> Vec<String> {
        self.state.screen().iter().map(|r| r.get().text()).collect()
    }

    fn scrollback(&self) -> Vec<String> {
        self.state
            .scrollback()
            .iter()
            .map(|r| r.get().text())
            .collect()
    }

    fn snapshot(&self) -> Vec<u8> {
        encode(&self.state).unwrap()
    }
}

fn small(cols: u16, rows: u16) -> Host {
    Host::new(&params().with_size(cols, rows))
}

#[test]
fn start_shows_a_blank_screen_of_the_param_size() {
    let host = small(10, 3);
    assert_eq!(host.screen(), vec![""; 3]);
    assert_eq!(
        (host.state.size().cols(), host.state.size().rows()),
        (10, 3)
    );
    assert_eq!(host.state.cursor(), crate::Cursor::new(0, 0, true));
}

#[test]
fn colors_and_bold_become_runs() {
    let mut host = small(40, 2);
    host.out(b"\x1b[1;31mERR\x1b[0m ok \x1b[38;2;1;2;3mx\x1b[48;5;200my\x1b[0m\x1b[2;3;4;7mz");
    let row = host.state.screen().get(0).unwrap().get();
    let want = vec![
        Run::new("ERR".into(), Color::Indexed(1), Color::Default, Attrs::BOLD),
        Run::new(" ok ".into(), Color::Default, Color::Default, Attrs::NONE),
        Run::new("x".into(), Color::Rgb(1, 2, 3), Color::Default, Attrs::NONE),
        Run::new(
            "y".into(),
            Color::Rgb(1, 2, 3),
            Color::Indexed(200),
            Attrs::NONE,
        ),
        Run::new(
            "z".into(),
            Color::Default,
            Color::Default,
            Attrs::DIM
                .with(Attrs::ITALIC)
                .with(Attrs::UNDERLINE)
                .with(Attrs::INVERSE),
        ),
    ];
    assert_eq!(row.runs(), want.as_slice());
    assert_eq!(row.text(), "ERR ok xyz");
    let y = row.runs().get(3).unwrap();
    assert_eq!(y.bg(), Color::Indexed(200));
    let z = row.runs().get(4).unwrap();
    assert!(z.attrs().contains(Attrs::INVERSE) && !z.attrs().contains(Attrs::BOLD));
}

#[test]
fn wide_cells_render_once() {
    let mut host = small(10, 1);
    host.out("界a".as_bytes());
    assert_eq!(host.screen(), vec!["界a"]);
}

/// `vt100` has no concealed attribute: `SGR 8` text shows as plain text.
/// TBD: blank concealed cells (needs an emulator that keeps the flag).
#[test]
fn concealed_text_is_not_blanked() {
    let mut host = small(10, 1);
    host.out(b"a\x1b[8mpw\x1b[0m");
    assert_eq!(host.screen(), vec!["apw"]);
}

#[test]
fn only_changed_rows_are_in_the_delta() {
    let mut host = small(10, 4);
    host.out(b"a\r\nb\r\nc");
    let mut ctx = Ctx::new();
    host.t
        .on_output(&mut ctx, &mut host.state, shell(), b"\x1b[2;1Hz");
    let change: ListChange<Value<Row>, Row> =
        decode(&encode(&Draft(host.state.screen())).unwrap()).unwrap();
    assert_eq!(change.truncate, None);
    assert!(change.push.is_empty());
    let edited: Vec<usize> = change.edits.iter().map(|(i, _)| *i).collect();
    assert_eq!(edited, vec![1]);
    assert_eq!(change.edits[0].1.text(), "z");
    assert!(!host.state.scrollback().has_draft());
    assert!(!host.state.title.has_draft());
}

#[test]
fn lines_scrolled_off_go_to_scrollback() {
    let mut host = small(10, 3);
    host.out(b"1\r\n2\r\n3\r\n4");
    host.out(b"\r\n5");
    assert_eq!(host.scrollback(), vec!["1", "2"]);
    assert_eq!(host.screen(), vec!["3", "4", "5"]);
    assert_eq!(host.replica.scrollback().len(), 2, "replica follows");
}

#[test]
fn shrinking_rows_pushes_the_top_into_scrollback() {
    let mut host = small(10, 3);
    host.out(b"1\r\n2\r\n3");
    host.out(b"");
    host.resize(10, 2).unwrap();
    assert_eq!(host.scrollback(), vec!["1"]);
    assert_eq!(host.screen(), vec!["2", "3"]);
    assert_eq!(host.state.size().rows(), 2);
    assert_eq!(
        host.effects.last(),
        Some(&Effect::Resize {
            proc: shell(),
            cols: 10,
            rows: 2
        })
    );
}

#[test]
fn scrollback_past_the_cap_keeps_the_newest_half() {
    let mut host = small(10, 1);
    let lines: String = (0..=SCROLLBACK_CAP).map(|i| format!("{i}\r\n")).collect();
    host.out(lines.as_bytes());
    let got = host.scrollback();
    assert_eq!(got.len(), SCROLLBACK_CAP / 2 + 1);
    assert_eq!(got.first().unwrap(), &(SCROLLBACK_CAP / 2).to_string());
    assert_eq!(got.last().unwrap(), &SCROLLBACK_CAP.to_string());
    assert_eq!(host.replica, host.state);
}

#[test]
fn clear_empties_scrollback_and_trims() {
    let mut host = small(10, 2);
    host.out(b"1\r\n2\r\n3");
    assert!(!host.scrollback().is_empty());
    host.out(b"\x1b[H\x1b[2J\x1b[3J$ ");
    assert!(host.scrollback().is_empty());
    assert_eq!(host.screen(), vec!["$", ""]);
    assert_eq!(
        host.effects,
        vec![
            Effect::Spawn {
                template: "shell".into(),
                cwd: "/".into(),
                size: Some((10, 2)),
            },
            Effect::Trim
        ]
    );
}

#[test]
fn clear_saved_split_across_chunks_still_trims() {
    let mut host = small(10, 2);
    host.out(b"x\x1b[2J\x1b[");
    host.out(b"3J");
    assert_eq!(host.effects.last(), Some(&Effect::Trim));
}

#[test]
fn clear_saved_with_text_on_screen_is_not_a_clean_point() {
    let mut host = small(10, 2);
    host.out(b"1\r\n2\r\n3\x1b[3J");
    assert!(host.scrollback().is_empty());
    assert_eq!(host.screen(), vec!["2", "3"]);
    assert!(!host.effects.contains(&Effect::Trim));
}

#[test]
fn clear_inside_alt_screen_keeps_scrollback() {
    let mut host = small(10, 2);
    host.out(b"1\r\n2\r\n3");
    host.out(b"\x1b[?1049h\x1b[2J\x1b[3J\x1b[Hvim");
    assert_eq!(host.scrollback(), vec!["1"]);
    assert_eq!(host.screen(), vec!["vim", ""]);
    host.out(b"\x1b[?1049l");
    assert_eq!(host.screen(), vec!["2", "3"]);
}

#[test]
fn title_follows_osc_and_reset() {
    let mut host = small(10, 2);
    host.out(b"\x1b]0;build\x07");
    assert_eq!(host.state.title(), "build");
    assert_eq!(host.replica.title(), "build");
    host.out(b"\x1b]2;\x07");
    assert_eq!(host.state.title(), "");
}

#[test]
fn input_before_the_shell_binds_is_refused() {
    let mut host = small(10, 2);
    let got = host.input(b"x");
    assert_eq!(got, Err(TermError::NoShell));
}

#[test]
fn input_after_the_shell_exits_is_refused() {
    let mut host = small(10, 2);
    host.out(b"$ ");
    host.step(|t, ctx, state| t.on_exit(ctx, state, shell(), ExitStatus::Code(0)));
    let got = host.input(b"x");
    assert_eq!(got, Err(TermError::NoShell));
}

#[test]
fn resize_before_the_shell_binds_reaches_the_pty_on_bind() {
    let mut host = small(10, 2);
    host.resize(20, 5).unwrap();
    assert_eq!(host.state.size().cols(), 20);
    assert_eq!(host.screen().len(), 5);
    host.out(b"$ ");
    let want = Effect::Resize {
        proc: shell(),
        cols: 20,
        rows: 5,
    };
    assert_eq!(host.effects.last(), Some(&want));
    host.out(b"more");
    assert_eq!(host.effects.iter().filter(|e| **e == want).count(), 1);
}

#[test]
fn output_of_another_proc_is_ignored() {
    let mut host = small(10, 2);
    host.out(b"$ ");
    let other = ProcRef::from_raw(2);
    host.step(|t, ctx, state| t.on_output(ctx, state, other, b"junk"));
    host.step(|t, ctx, state| t.on_exit(ctx, state, other, ExitStatus::Code(1)));
    assert_eq!(host.screen(), vec!["$", ""]);
    assert!(!host.effects.contains(&Effect::Finish));
}

#[test]
fn cursor_position_report_is_written_to_the_shell() {
    let mut host = small(10, 3);
    host.out(b"ab\x1b[6n");
    assert_eq!(
        host.effects.last(),
        Some(&Effect::Write {
            proc: shell(),
            bytes: b"\x1b[1;3R".to_vec()
        })
    );
}

#[test]
fn gap_resets_the_screen_and_keeps_scrollback() {
    let mut host = small(10, 2);
    host.out(b"\x1b]0;t\x071\r\n2\r\n3");
    host.step(|t, ctx, state| t.on_gap(ctx, state));
    assert_eq!(host.scrollback(), vec!["1"]);
    assert_eq!(host.screen(), vec!["", ""]);
    assert_eq!(host.state.title(), "");
    host.out(b"ok");
    assert_eq!(host.screen(), vec!["ok", ""]);
    assert_eq!(host.replica, host.state);
}

/// Output with styles, scrolling, a title, a clear, the alternate screen and a split UTF-8 char.
fn session_bytes() -> Vec<u8> {
    let mut bytes = Vec::new();
    for i in 0..30 {
        bytes.extend(format!("\x1b[3{}mline {i}\x1b[0m\r\n", i % 8).as_bytes());
    }
    bytes.extend(b"\x1b]0;demo\x07\x1b[H\x1b[2J\x1b[3J$ ls\r\n");
    bytes.extend("界 wide\r\n".as_bytes());
    bytes.extend(b"\x1b[?1049h\x1b[5;5Hvim\x1b[?2026hsync\x1b[?2026l\x1b[?1049l");
    for i in 0..12 {
        bytes.extend(format!("tail {i}\r\n").as_bytes());
    }
    bytes
}

fn replay(chunk: usize) -> Host {
    let mut host = small(12, 4);
    for part in session_bytes().chunks(chunk) {
        host.out(part);
    }
    host
}

#[test]
fn replay_gives_the_same_state() {
    let whole = replay(usize::MAX);
    for chunk in [1, 2, 3, 7, 64] {
        let got = replay(chunk);
        assert_eq!(got.snapshot(), whole.snapshot(), "case=chunk {chunk}");
        assert_eq!(got.replica, got.state, "case=replica chunk {chunk}");
    }
    assert_eq!(whole.state.title(), "demo");
    assert_eq!(whole.scrollback().first().unwrap(), "$ ls");
}

#[test]
fn resumed_nudges_the_pty_of_a_live_shell_only() {
    let mut host = small(10, 2);
    host.step(|t, ctx, state| t.resumed(ctx, state));
    assert_eq!(host.effects.len(), 1, "unbound: only the spawn of start");
    host.out(b"$ ");
    host.step(|t, ctx, state| t.resumed(ctx, state));
    let nudge = Effect::Resize {
        proc: shell(),
        cols: 10,
        rows: 2,
    };
    assert_eq!(host.effects.last(), Some(&nudge));
    host.step(|t, ctx, state| t.on_exit(ctx, state, shell(), ExitStatus::Code(0)));
    let before = host.effects.len();
    host.step(|t, ctx, state| t.resumed(ctx, state));
    assert_eq!(host.effects.len(), before, "exited: no nudge");
}

/// Sends every call through the generated runner adapter, as the worker host does.
struct Dispatcher(std::cell::RefCell<Host>);

impl lattice_app::Caller for Dispatcher {
    fn call(&self, action: Vec<u8>) -> lattice_app::RawTicket {
        use lattice_app::Dispatch;
        let mut host = self.0.borrow_mut();
        let mut got = Ok(Vec::new());
        host.step(|t, ctx, state| {
            got = crate::TerminalApiRunner::<Terminal>::dispatch(t, ctx, state, &action);
        });
        lattice_app::RawTicket::ready(got.map_err(lattice_app::RawCallError::Refused))
    }
}

#[test]
fn resize_through_the_client_returns_the_size_in_effect() {
    let mut host = small(10, 2);
    host.out(b"$ ");
    let client = crate::TerminalApiClient::new(Dispatcher(std::cell::RefCell::new(host)));
    let got = client.resize(Resize::new(30, 4)).try_take();
    assert_eq!(got, Some(Ok(Size::new(30, 4))));
    let got = client.resize(Resize::new(0, 4)).try_take();
    assert_eq!(
        got,
        Some(Err(lattice_app::CallError::App(TermError::BadSize)))
    );
    assert_eq!(
        client.input(Input::new(b"ls\r".to_vec())).try_take(),
        Some(Ok(()))
    );
}

#[test]
fn clear_inside_a_synchronized_update_is_not_a_clean_point() {
    let mut host = small(10, 2);
    host.out(b"1\r\n2\r\n3");
    host.out(b"\x1b[?2026h\x1b[H\x1b[2J\x1b[3J");
    assert_eq!(host.scrollback(), vec!["1"]);
    host.out(b"\x1b[?2026l");
    assert!(!host.effects.contains(&Effect::Trim));
}

#[test]
fn growing_after_scrolling_adds_rows_below() {
    let mut host = small(10, 2);
    host.out(b"1\r\n2\r\n3");
    host.resize(10, 4).unwrap();
    assert_eq!(host.scrollback(), vec!["1"]);
    assert_eq!(host.screen(), vec!["2", "3", "", ""]);
    assert_eq!(host.state.cursor().position(), (1, 1));
}

#[test]
fn device_attributes_are_answered() {
    let mut host = small(10, 2);
    host.out(b"\x1b[c");
    assert_eq!(
        host.effects.last(),
        Some(&Effect::Write {
            proc: shell(),
            bytes: b"\x1b[?6c".to_vec()
        })
    );
}
