//! The built-in apps, built to wasm32 and run as packages (D42, D43).
//!
//! Echo runs through the worker's apps runner, and its viewer module follows the state and
//! turns a submit into a `send`; the terminal runs beside its native build to show the guest
//! gives the same state and the same view tree. The modules come from the packages
//! `lattice-builtin-apps` builds and embeds, so this tests what a deck ships.

// Inside a `cfg(test)` module so the helpers get the test lint allowances (clippy.toml).
#[cfg(test)]
mod apps {
    use std::sync::Arc;

    use lattice_app::{AppId, AppsAction, BAD_ACTION, InstanceId, Mode, PackageHash};
    use lattice_app_echo::{EchoApiAction, EchoError, EchoParams, EchoState, Message};
    use lattice_builtin_apps::Builtin;
    use lattice_package::Package;
    use lattice_wasm::{Limits, TRAP, WasmApp, engine};
    use lattice_worker::{AppsEffect, AppsRunner, BootEpoch, Installed, StateSync};

    fn builtin(id: &str) -> (&'static Builtin, Package) {
        let b = lattice_builtin_apps::by_id(id).unwrap();
        (b, Package::verify(&b.package(), b.bytes).unwrap())
    }

    fn echo_hash() -> PackageHash {
        builtin(lattice_app_echo::APP_ID).0.package()
    }

    fn echo_id() -> AppId {
        AppId::try_new(lattice_app_echo::APP_ID).unwrap()
    }

    fn runner_with(limits: Limits) -> AppsRunner {
        let (b, package) = builtin(lattice_app_echo::APP_ID);
        let app = WasmApp::load(&engine().unwrap(), &package.runner, Limits::default())
            .unwrap()
            .with_limits(limits);
        let runner = Arc::new(move |params: &[u8], mode: Mode| app.instantiate(params, mode));
        let mut apps = AppsRunner::new(BootEpoch::from_unix_millis(0));
        apps.install(b.package(), Installed::new(package.manifest, runner));
        apps
    }

    fn create(apps: &mut AppsRunner) -> Result<InstanceId, String> {
        let params = lattice_state::encode(&EchoParams::default()).unwrap();
        let action = AppsAction::Create {
            app: echo_id(),
            package: Some(echo_hash()),
            params,
            session: "test".to_owned(),
        };
        apps.on_apps_action(action).1.map(|iid| iid.unwrap())
    }

    fn send(text: &str) -> Vec<u8> {
        lattice_state::encode(&EchoApiAction::Send(Message::new(text))).unwrap()
    }

    #[test]
    fn echo_as_wasm_answers_send_with_its_seq_and_syncs_the_line() {
        let mut apps = runner_with(Limits::default());
        let iid = create(&mut apps).unwrap();

        let (effects, reply) = apps.on_instance_action(iid, &send("hello"));
        assert_eq!(reply, Ok(lattice_state::encode(&1u64).unwrap()));
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, AppsEffect::Delta { iid: i, .. } if *i == iid)),
            "the committed line goes out as a delta: {effects:?}"
        );

        let Some(StateSync::Full { bytes, .. }) = apps.state_sync(iid, None) else {
            panic!("a new reader gets the full state");
        };
        let state: EchoState = lattice_state::decode(&bytes).unwrap();
        let lines: Vec<_> = state
            .lines()
            .iter()
            .map(|l| l.get().text().to_owned())
            .collect();
        assert_eq!(lines, ["hello"]);
    }

    #[test]
    fn a_refused_send_crosses_as_the_app_error_and_commits_nothing() {
        let mut apps = runner_with(Limits::default());
        let iid = create(&mut apps).unwrap();
        let (effects, reply) = apps.on_instance_action(iid, &send("   "));
        let err = reply.unwrap_err();
        assert_eq!(err.code(), "empty");
        let detail: EchoError = lattice_state::decode(err.detail()).unwrap();
        assert!(matches!(detail, EchoError::Empty));
        assert!(
            !effects
                .iter()
                .any(|e| matches!(e, AppsEffect::Delta { .. }))
        );
    }

    #[test]
    fn bytes_that_are_no_action_are_refused_by_the_guest() {
        let mut apps = runner_with(Limits::default());
        let iid = create(&mut apps).unwrap();
        let (_, reply) = apps.on_instance_action(iid, &[0xff, 0xff]);
        assert_eq!(reply.unwrap_err().code(), BAD_ACTION);
    }

    #[test]
    fn a_runner_out_of_fuel_fails_to_create() {
        let mut apps = runner_with(Limits {
            fuel_per_call: 10,
            ..Limits::default()
        });
        assert_eq!(create(&mut apps), Err(TRAP.to_owned()));
    }

    #[test]
    fn a_module_with_imports_does_not_load() {
        // (module (import "env" "f" (func))): a minimal module with one import.
        let bytes = [
            0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // magic, version
            0x01, 0x04, 0x01, 0x60, 0x00, 0x00, // type section: () -> ()
            0x02, 0x09, 0x01, 0x03, b'e', b'n', b'v', 0x01, b'f', 0x00, 0x00, // import env.f
        ];
        let err = WasmApp::load(&engine().unwrap(), &bytes, Limits::default()).unwrap_err();
        assert_eq!(err.code(), "wasm.import");
    }

    #[test]
    fn the_terminal_as_wasm_gives_the_same_state_as_its_native_build() {
        use lattice_app::{Effect, Input, ProcRef, hosted};
        use lattice_app_terminal::{TermParams, TermState, Terminal, TerminalApiRunner};

        let (_, package) = builtin(lattice_app_terminal::APP_ID);
        let app = WasmApp::load(&engine().unwrap(), &package.runner, Limits::default()).unwrap();
        let params =
            lattice_state::encode(&TermParams::new("/tmp".to_owned()).with_size(40, 5)).unwrap();
        let (mut guest, started) = app.instantiate(&params, Mode::Live).unwrap();
        let (mut native, native_started) =
            hosted::<TerminalApiRunner<Terminal>>(&params, Mode::Live).unwrap();
        assert_eq!(started, native_started);
        assert!(
            started
                .effects
                .iter()
                .any(|e| matches!(e, Effect::Spawn { template, .. } if template == "shell"))
        );

        let proc = ProcRef::from_raw(1);
        let output = b"hello \x1b[1;31mred\x1b[0m\r\nline two\r\n$ ";
        let step = guest.step(Input::Output(proc, output), Mode::Live);
        assert_eq!(step, native.step(Input::Output(proc, output), Mode::Live));
        assert!(step.delta.is_some());
        assert_eq!(guest.snapshot(), native.snapshot());

        let state: TermState = lattice_state::decode(&guest.snapshot()).unwrap();
        let first = state
            .screen()
            .iter()
            .next()
            .map(|r| r.get().text())
            .unwrap();
        assert!(first.starts_with("hello red"), "{first:?}");
    }

    fn viewer(id: &str) -> lattice_wasm::WasmViewer {
        let (_, package) = builtin(id);
        lattice_wasm::WasmViewerModule::load(&engine().unwrap(), &package.viewer, Limits::default())
            .unwrap()
            .instantiate()
    }

    #[test]
    fn the_echo_viewer_as_wasm_follows_the_state_and_sends_what_is_submitted() {
        use lattice_app::ReplicaEvent;
        use lattice_app::view::{DynViewer, EventKind, UiEvent, Widget};

        let mut apps = runner_with(Limits::default());
        let iid = create(&mut apps).unwrap();
        let mut view = viewer(lattice_app_echo::APP_ID);
        let Some(StateSync::Full { version, bytes }) = apps.state_sync(iid, None) else {
            panic!("full state");
        };
        assert_eq!(view.load(version, &bytes), ReplicaEvent::Changed);

        // The field (key 4) submits; the viewer answers with the same bytes a client sends.
        let field = view
            .render()
            .nodes()
            .iter()
            .find(|n| matches!(n.widget, Widget::Input { .. }))
            .map(|n| n.key)
            .unwrap();
        let submit = UiEvent {
            key: field,
            kind: EventKind::Submit("from the viewer".to_owned()),
        };
        let actions = view.handle(&submit);
        assert_eq!(actions, [send("from the viewer")]);

        let (effects, reply) = apps.on_instance_action(iid, &actions[0]);
        assert!(reply.is_ok());
        let (version, delta) = effects
            .iter()
            .find_map(|e| match e {
                AppsEffect::Delta { version, bytes, .. } => Some((*version, bytes.clone())),
                _ => None,
            })
            .unwrap();
        assert_eq!(view.apply(version, &delta), ReplicaEvent::Changed);
        assert_eq!(view.version(), Some(version));
        assert_eq!(
            view.apply(version, &delta),
            ReplicaEvent::Resync,
            "same version twice"
        );
        let shown = format!("{:?}", view.render());
        assert!(shown.contains("from the viewer"), "{shown}");
        assert!(!view.is_dead());
    }

    #[test]
    fn the_terminal_viewer_as_wasm_draws_what_its_native_build_draws() {
        use lattice_app::view::{DynViewer, ViewerCore};
        use lattice_app::{Input, ProcRef, hosted};
        use lattice_app_terminal::{TermParams, TermViewer, Terminal, TerminalApiRunner};

        let params =
            lattice_state::encode(&TermParams::new("/tmp".to_owned()).with_size(40, 5)).unwrap();
        let (mut runner, _) = hosted::<TerminalApiRunner<Terminal>>(&params, Mode::Live).unwrap();
        let output = b"hello \x1b[1;31mred\x1b[0m\r\n$ ";
        runner.step(Input::Output(ProcRef::from_raw(1), output), Mode::Live);
        let state = runner.snapshot();

        let mut wasm = viewer(lattice_app_terminal::APP_ID);
        let mut native = ViewerCore::<TermViewer>::default();
        assert_eq!(wasm.load(3, &state), native.load(3, &state));
        assert_eq!(wasm.render(), native.render());
        assert!(!wasm.render().nodes().is_empty());
    }

    #[test]
    fn a_viewer_out_of_fuel_is_dead_and_a_runner_module_is_no_viewer() {
        use lattice_app::ReplicaEvent;
        use lattice_app::view::DynViewer;
        use lattice_wasm::WasmViewerModule;

        let (_, package) = builtin(lattice_app_echo::APP_ID);
        let engine = engine().unwrap();
        let module = WasmViewerModule::load(&engine, &package.viewer, Limits::default()).unwrap();
        let tight = Limits {
            fuel_per_call: 10,
            ..Limits::default()
        };
        let mut view = module.with_limits(tight).instantiate();
        let state = lattice_state::encode(&EchoState::default()).unwrap();
        assert_eq!(view.load(0, &state), ReplicaEvent::Resync);
        assert!(view.is_dead());
        assert!(view.render().nodes().is_empty());

        let err = WasmViewerModule::load(&engine, &package.runner, Limits::default()).unwrap_err();
        assert_eq!(err.code(), "wasm.export");
    }
}
