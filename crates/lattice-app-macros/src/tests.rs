use super::*;

/// The terminal interface with `extra` methods appended.
fn terminal(extra: Tokens) -> ItemTrait {
    syn::parse_quote! {
        pub trait TerminalApi: App<State = TermState, Params = TermParams, Error = TermError> {
            /// Keystrokes.
            fn input(&mut self, ctx: &mut Ctx, state: &mut TermState, req: Input) -> Result<(), TermError>;
            fn resize(&mut self, ctx: &mut Ctx, state: &mut TermState, req: Resize) -> Result<Size, TermError>;
            #extra
        }
    }
}

fn expand(input: &ItemTrait) -> Result<String, Error> {
    interface_impl(input, &quote! { ::lattice_app }).map(|t| t.to_string())
}

/// The error of `#[interface]` on the terminal trait with `extra` methods, or `None`.
fn error(extra: Tokens) -> Option<String> {
    expand(&terminal(extra)).err().map(|e| e.to_string())
}

#[test]
fn expands_action_enum_runner_and_client_in_declaration_order() -> Result<(), Error> {
    let out = expand(&terminal(quote! {}))?;
    assert!(out.contains("pub enum TerminalApiAction"), "{out}");
    let input = out.find("Input (Input)").unwrap_or(usize::MAX);
    let resize = out.find("Resize (Resize)").unwrap_or(0);
    assert!(input < resize, "variants follow declaration order: {out}");
    assert!(
        out.contains(":: lattice_app :: Dispatch for TerminalApiRunner < T >"),
        "{out}"
    );
    assert!(
        out.contains("< T as TerminalApi > :: resize (app , ctx , state , req)"),
        "{out}"
    );
    assert!(
        out.contains("< _ , < T as :: lattice_app :: App > :: Error >"),
        "reply checks the error is the app's: {out}"
    );
    assert!(
        out.contains(
            "pub fn resize (& self , req : Resize) -> :: lattice_app :: Ticket < Size , TermError >"
        ),
        "{out}"
    );
    assert!(out.contains("impl < C : :: lattice_app :: Caller > TerminalApiClient < C >"));
    assert!(
        out.contains("\" Keystrokes.\""),
        "method docs move to the client: {out}"
    );
    assert!(out.contains("\"Calls `resize`.\""), "default doc: {out}");
    assert!(out.contains("serde (crate = \"::lattice_app::__private::serde\")"));
    Ok(())
}

#[test]
fn crate_attribute_overrides_the_path() -> Result<(), Error> {
    let krate = crate_path(quote! { crate = "crate" })?;
    let out = interface_impl(&terminal(quote! {}), &krate)?.to_string();
    assert!(
        out.contains("crate :: Dispatch for TerminalApiRunner"),
        "{out}"
    );
    assert!(!out.contains("lattice_app"), "{out}");
    let bad = crate_path(quote! { core = "x" })
        .err()
        .map(|e| e.to_string());
    assert_eq!(
        bad.as_deref(),
        Some("#[interface]: expected `crate = \"path\"`")
    );
    Ok(())
}

#[test]
fn method_names_become_upper_camel_variants() {
    let cases = [
        ("input", "Input"),
        ("set_title", "SetTitle"),
        ("r#type", "Type"),
        ("a__b_", "AB"),
    ];
    for (name, want) in cases {
        assert_eq!(upper_camel(name), want, "case={name}");
    }
}

#[test]
fn refuses_bad_reducer_shapes() {
    let want = "expected `fn bad(&mut self, ctx: &mut Ctx, state: &mut State, req: Req) -> Result<Res, Error>`";
    let cases = [
        (
            "no receiver",
            quote! { fn bad(ctx: &mut Ctx, state: &mut S, req: R) -> Result<(), E>; },
        ),
        (
            "shared self",
            quote! { fn bad(&self, ctx: &mut Ctx, state: &mut S, req: R) -> Result<(), E>; },
        ),
        (
            "shared ctx",
            quote! { fn bad(&mut self, ctx: &Ctx, state: &mut S, req: R) -> Result<(), E>; },
        ),
        (
            "not ctx",
            quote! { fn bad(&mut self, ctx: &mut Context, state: &mut S, req: R) -> Result<(), E>; },
        ),
        (
            "shared state",
            quote! { fn bad(&mut self, ctx: &mut Ctx, state: &S, req: R) -> Result<(), E>; },
        ),
        (
            "borrowed req",
            quote! { fn bad(&mut self, ctx: &mut Ctx, state: &mut S, req: &R) -> Result<(), E>; },
        ),
        (
            "no req",
            quote! { fn bad(&mut self, ctx: &mut Ctx, state: &mut S) -> Result<(), E>; },
        ),
        (
            "no result",
            quote! { fn bad(&mut self, ctx: &mut Ctx, state: &mut S, req: R); },
        ),
        (
            "not result",
            quote! { fn bad(&mut self, ctx: &mut Ctx, state: &mut S, req: R) -> Option<()>; },
        ),
        (
            "default body",
            quote! { fn bad(&mut self, ctx: &mut Ctx, state: &mut S, req: R) -> Result<(), E> { Ok(()) } },
        ),
        (
            "generic",
            quote! { fn bad<X>(&mut self, ctx: &mut Ctx, state: &mut S, req: X) -> Result<(), E>; },
        ),
        (
            "async",
            quote! { async fn bad(&mut self, ctx: &mut Ctx, state: &mut S, req: R) -> Result<(), E>; },
        ),
    ];
    for (name, extra) in cases {
        let got = error(extra).unwrap_or_default();
        assert!(got.contains(want), "case={name}: {got}");
    }
}

#[test]
fn refuses_self_types_other_items_and_a_missing_app_bound() {
    let got = error(quote! {
        fn bad(&mut self, ctx: &mut Ctx, state: &mut S, req: R) -> Result<(), Self::Error>;
    });
    assert!(got.unwrap_or_default().contains("may not name `Self`"));
    let got = error(quote! { type Extra; });
    assert!(
        got.unwrap_or_default()
            .contains("only reducer methods are allowed")
    );
    let input: ItemTrait = syn::parse_quote! {
        trait NoApp: Send {
            fn input(&mut self, ctx: &mut Ctx, state: &mut S, req: R) -> Result<(), E>;
        }
    };
    let got = expand(&input).err().map(|e| e.to_string());
    assert_eq!(
        got.as_deref(),
        Some("#[interface]: the trait needs an `App<..>` supertrait")
    );
    let input: ItemTrait = syn::parse_quote! { trait G<X>: App {} };
    let got = expand(&input).err().map(|e| e.to_string());
    assert_eq!(
        got.as_deref(),
        Some("#[interface]: generic traits are not supported")
    );
    assert_eq!(error(quote! {}), None);
}
