//! `#[interface]` for an app's interface trait (decision D36).
//!
//! Use it through `lattice-app` (`#[lattice_app::interface]`). Generated code names
//! `::lattice_app` and `::lattice_app::__private::serde`, so an app needs no direct path to
//! either for the generated types.
//!
//! Contract: `docs/architecture/06-apps.md` › Interface.
//! Ported from realtime-workspace rtf-macros `#[rtf::interface]` (RFC-0005 §4, §7), without
//! drafts: Lattice viewers never predict.
//!
//! # Key items
//!
//! - [`macro@interface`]: turns `trait XApi: App<..>` into `XApiAction`, `XApiRunner<T>` and
//!   `XApiClient<C>`.

use proc_macro::TokenStream;
use proc_macro2::{TokenStream as Tokens, TokenTree};
use quote::{ToTokens, format_ident, quote};
use syn::spanned::Spanned;
use syn::{
    Error, FnArg, GenericArgument, Ident, ItemTrait, PathArguments, ReturnType, TraitItem, Type,
    TypeParamBound, parse_macro_input,
};

/// Turns an app's interface trait into its wire action enum, a runner adapter and a typed
/// client (06-apps.md, Interface). Every method is a reducer shaped
/// `fn name(&mut self, ctx: &mut Ctx, state: &mut State, req: Req) -> Result<Res, Error>`,
/// and the trait has an `App<..>` supertrait. `Req` and `Res` are any serde types (`()` too);
/// `Error` is the app's `App::Error`. Types may not name `Self`: the client repeats them.
///
/// For `trait TerminalApi: App<..>` it generates, with the visibility of the trait:
///
/// - `TerminalApiAction`: one variant per method, in declaration order, named after the method
///   in `UpperCamelCase`. Encoded with postcard, so the variant index is the wire tag:
///   **append new methods at the end**, never reorder or remove one; old clients keep working.
/// - `TerminalApiRunner<T>`: implements `lattice_app::Dispatch` for any `T: TerminalApi`; the
///   host registers `TerminalApiRunner<Terminal>`. It decodes an action, calls the method, and
///   encodes `Res`, or the error as `lattice_app::AppError` (`{code, detail}`).
/// - `TerminalApiClient<C: lattice_app::Caller>`: one method per reducer, returning
///   `lattice_app::Ticket<Res, Error>`.
///
/// ```ignore
/// #[lattice_app::interface]
/// pub trait TerminalApi: App<State = TermState, Params = TermParams, Error = TermError> {
///     fn resize(&mut self, ctx: &mut Ctx, state: &mut TermState, req: Resize) -> Result<Size, TermError>;
/// }
/// ```
///
/// `#[interface(crate = "path")]` names `lattice-app` when it is not `::lattice_app`.
#[proc_macro_attribute]
pub fn interface(attr: TokenStream, item: TokenStream) -> TokenStream {
    let krate = match crate_path(attr.into()) {
        Ok(k) => k,
        Err(e) => return e.into_compile_error().into(),
    };
    let input = parse_macro_input!(item as ItemTrait);
    interface_impl(&input, &krate)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

/// `::lattice_app`, or the path of `crate = "…"`.
fn crate_path(attr: Tokens) -> Result<Tokens, Error> {
    let mut path = quote! { ::lattice_app };
    if attr.is_empty() {
        return Ok(path);
    }
    let parser = syn::meta::parser(|meta| {
        if !meta.path.is_ident("crate") {
            return Err(meta.error("#[interface]: expected `crate = \"path\"`"));
        }
        let parsed: syn::Path = meta.value()?.parse::<syn::LitStr>()?.parse()?;
        path = parsed.into_token_stream();
        Ok(())
    });
    syn::parse::Parser::parse2(parser, attr)?;
    Ok(path)
}

/// One reducer of the trait.
struct Reducer {
    method: Ident,
    variant: Ident,
    req: Type,
    res: Type,
    err: Type,
    docs: Vec<syn::Attribute>,
}

fn interface_impl(input: &ItemTrait, krate: &Tokens) -> Result<Tokens, Error> {
    check_app_bound(input)?;
    if !input.generics.params.is_empty() {
        return Err(Error::new(
            input.generics.span(),
            "#[interface]: generic traits are not supported",
        ));
    }
    let reducers = input
        .items
        .iter()
        .map(|item| match item {
            TraitItem::Fn(f) => reducer(f),
            other => Err(Error::new(
                other.span(),
                "#[interface]: only reducer methods are allowed",
            )),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let action = action_enum(input, &reducers, krate);
    let runner = runner(input, &reducers, krate);
    let client = client(input, &reducers, krate);
    Ok(quote! {
        #input
        #action
        #runner
        #client
    })
}

/// `XAction`: one variant per reducer, postcard-encoded by declaration order.
fn action_enum(input: &ItemTrait, reducers: &[Reducer], krate: &Tokens) -> Tokens {
    let (name, vis) = (&input.ident, &input.vis);
    let action = format_ident!("{}Action", name);
    let serde = quote! { #krate::__private::serde };
    let serde_path = serde.to_string().replace(' ', "");
    let variants = reducers.iter().map(|r| {
        let (variant, req) = (&r.variant, &r.req);
        let doc = format!("Request of `{}`.", r.method);
        quote! { #[doc = #doc] #variant(#req) }
    });
    let doc = format!(
        "Actions of [`{name}`] on the wire: one variant per method, postcard-encoded by \
         declaration order. New methods are appended."
    );
    quote! {
        #[doc = #doc]
        #[derive(::std::fmt::Debug, #serde::Serialize, #serde::Deserialize)]
        #[serde(crate = #serde_path)]
        #vis enum #action {
            #(#variants),*
        }
    }
}

/// `XRunner<T>`: the `Dispatch` adapter the host registers.
fn runner(input: &ItemTrait, reducers: &[Reducer], krate: &Tokens) -> Tokens {
    let (name, vis) = (&input.ident, &input.vis);
    let action = format_ident!("{}Action", name);
    let runner = format_ident!("{}Runner", name);
    let arms = reducers.iter().map(|r| {
        let (variant, method) = (&r.variant, &r.method);
        quote! {
            #action::#variant(req) => #krate::__private::reply::<_, <T as #krate::App>::Error>(
                <T as #name>::#method(app, ctx, state, req),
            )
        }
    });
    let doc = format!(
        "Runner adapter of [`{name}`]: the host registers `{runner}::<App>`; it decodes an \
         action, calls the method, and encodes the reply or the error."
    );
    quote! {
        #[doc = #doc]
        #[allow(dead_code, reason = "generated: a crate may use only some of the parts")]
        #vis struct #runner<T>(::std::marker::PhantomData<fn() -> T>);

        impl<T: #name> #krate::Dispatch for #runner<T> {
            type App = T;

            fn dispatch(
                app: &mut T,
                ctx: &mut #krate::Ctx,
                state: &mut <T as #krate::App>::State,
                action: &[u8],
            ) -> ::std::result::Result<::std::vec::Vec<u8>, #krate::AppError> {
                match #krate::__private::decode_action::<#action>(action)? {
                    #(#arms),*
                }
            }
        }
    }
}

/// `XClient<C>`: typed methods over a `Caller`.
fn client(input: &ItemTrait, reducers: &[Reducer], krate: &Tokens) -> Tokens {
    let (name, vis) = (&input.ident, &input.vis);
    let action = format_ident!("{}Action", name);
    let client = format_ident!("{}Client", name);
    let methods = reducers.iter().map(|r| {
        let (variant, method, req, res, err) = (&r.variant, &r.method, &r.req, &r.res, &r.err);
        let docs = if r.docs.is_empty() {
            let doc = format!("Calls `{method}`.");
            quote! { #[doc = #doc] }
        } else {
            let docs = &r.docs;
            quote! { #(#docs)* }
        };
        quote! {
            #docs
            #vis fn #method(&self, req: #req) -> #krate::Ticket<#res, #err> {
                #krate::__private::call(&self.caller, &#action::#variant(req))
            }
        }
    });
    let doc = format!(
        "Typed client of [`{name}`] over a [`Caller`]({krate}::Caller) bound to one instance.",
        krate = krate.to_string().replace(' ', "")
    );
    quote! {
        #[doc = #doc]
        #[derive(::std::fmt::Debug, ::std::clone::Clone)]
        #[allow(dead_code, reason = "generated: a crate may use only some of the parts")]
        #vis struct #client<C> {
            caller: C,
        }

        #[allow(dead_code, reason = "generated: a crate may use only some of the parts")]
        impl<C: #krate::Caller> #client<C> {
            /// A client sending through `caller` (bound to one app instance).
            #vis fn new(caller: C) -> Self {
                Self { caller }
            }

            #(#methods)*
        }
    }
}

/// The trait needs an `App<..>` supertrait: hooks and associated types live there.
fn check_app_bound(input: &ItemTrait) -> Result<(), Error> {
    let has_app = input.supertraits.iter().any(|bound| {
        matches!(bound, TypeParamBound::Trait(t)
            if t.path.segments.last().is_some_and(|s| s.ident == "App"))
    });
    if has_app {
        Ok(())
    } else {
        Err(Error::new(
            input.ident.span(),
            "#[interface]: the trait needs an `App<..>` supertrait",
        ))
    }
}

/// Checks one method's shape and extracts its types.
fn reducer(f: &syn::TraitItemFn) -> Result<Reducer, Error> {
    let sig = &f.sig;
    let shape = || {
        Error::new(
            sig.span(),
            format!(
                "#[interface]: expected `fn {}(&mut self, ctx: &mut Ctx, state: &mut State, \
                 req: Req) -> Result<Res, Error>`",
                sig.ident
            ),
        )
    };
    let req = request_type(f).ok_or_else(shape)?;
    let (res, err) = result_types(&sig.output).ok_or_else(shape)?;
    for ty in [req, &res, &err] {
        if names_self(ty.to_token_stream()) {
            return Err(Error::new(
                ty.span(),
                "#[interface]: request, result, and error types may not name `Self`; \
                 write the concrete type",
            ));
        }
    }
    Ok(Reducer {
        method: sig.ident.clone(),
        variant: format_ident!("{}", upper_camel(&sig.ident.to_string())),
        req: req.clone(),
        res,
        err,
        docs: f
            .attrs
            .iter()
            .filter(|a| a.path().is_ident("doc"))
            .cloned()
            .collect(),
    })
}

/// `Req` of `fn(&mut self, ctx: &mut Ctx, state: &mut State, req: Req)`; `None` for any
/// other shape (default body, generics, `async`, `unsafe`, borrowed `req`, …).
fn request_type(f: &syn::TraitItemFn) -> Option<&Type> {
    let sig = &f.sig;
    if f.default.is_some()
        || !sig.generics.params.is_empty()
        || sig.asyncness.is_some()
        || sig.unsafety.is_some()
        || sig.variadic.is_some()
    {
        return None;
    }
    let mut inputs = sig.inputs.iter();
    let receiver_ok = matches!(inputs.next(), Some(FnArg::Receiver(r))
        if r.reference.is_some() && r.mutability.is_some() && r.colon_token.is_none());
    let args: Vec<&Type> = inputs
        .map(|a| match a {
            FnArg::Typed(t) => Some(&*t.ty),
            FnArg::Receiver(_) => None,
        })
        .collect::<Option<_>>()?;
    let [ctx, state, req] = args.as_slice() else {
        return None;
    };
    let ctx_ok = matches!(ctx, Type::Reference(r)
        if r.mutability.is_some() && last_ident(&r.elem).as_deref() == Some("Ctx"));
    let state_ok = matches!(state, Type::Reference(r) if r.mutability.is_some());
    let req_ok = !matches!(req, Type::Reference(_));
    (receiver_ok && ctx_ok && state_ok && req_ok).then_some(*req)
}

/// True when `tokens` contain the `Self` keyword at any depth.
fn names_self(tokens: Tokens) -> bool {
    tokens.into_iter().any(|t| match t {
        TokenTree::Ident(i) => i == "Self",
        TokenTree::Group(g) => names_self(g.stream()),
        TokenTree::Punct(_) | TokenTree::Literal(_) => false,
    })
}

/// `snake_case` (or `r#raw`) to `UpperCamelCase`: `set_title` → `SetTitle`.
fn upper_camel(name: &str) -> String {
    name.trim_start_matches("r#")
        .split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            chars
                .next()
                .map(|c| c.to_uppercase().chain(chars).collect::<String>())
                .unwrap_or_default()
        })
        .collect()
}

/// Last path segment of a type, e.g. `Ctx` for `lattice_app::Ctx`.
fn last_ident(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
        _ => None,
    }
}

/// `Res` and `Error` of `-> Result<Res, Error>`.
fn result_types(output: &ReturnType) -> Option<(Type, Type)> {
    let ReturnType::Type(_, ty) = output else {
        return None;
    };
    let Type::Path(p) = &**ty else {
        return None;
    };
    let last = p.path.segments.last()?;
    if last.ident != "Result" {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &last.arguments else {
        return None;
    };
    let mut types = args.args.iter().filter_map(|a| match a {
        GenericArgument::Type(t) => Some(t.clone()),
        _ => None,
    });
    let pair = (types.next()?, types.next()?);
    types.next().is_none().then_some(pair)
}

#[cfg(test)]
mod tests;
