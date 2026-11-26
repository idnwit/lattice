//! `#[derive(Synced)]` for app state structs.
//!
//! Use it through `lattice-state` (`use lattice_state::Synced;`): the trait and the derive
//! share the name, as serde's do. Generated code names `::lattice_state` and
//! `::lattice_state::__private::serde`, so an app needs no direct `serde` path for the
//! generated change type.
//!
//! Contract: `docs/architecture/06-apps.md` › State.
//! Ported from realtime-workspace rtf-macros (RFC-0005 §6); only the `Synced` derive.
//!
//! # Key items
//!
//! - [`macro@Synced`]: implements `lattice_state::Synced` for a struct with named fields and
//!   emits its `<Name>Change` type.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as Tokens;
use quote::{format_ident, quote};
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Error, Fields, Ident, Type, Visibility, parse_macro_input};

/// Implements `Synced` for a struct whose named fields are all `Synced`. The struct also
/// derives `Serialize` and `Deserialize` (its snapshot), and usually `Default`.
///
/// Emits, with the visibility of the struct:
///
/// - `<Name>Change`: one `Option<<F as Synced>::Change>` per field, with the struct's
///   visibility. Every field is always encoded (`None` when that field has no draft), in
///   field order, so the change decodes under postcard.
/// - `impl Synced`: `has_draft`, `write_change` (encodes as `<Name>Change`, reading the fields
///   in place), `commit`, `rollback` and `apply` ask each field, a fixed count; `rev` is the
///   saturating sum of the fields' (it grows when any field's does).
///
/// ```ignore
/// #[derive(Default, Serialize, Deserialize, Synced)]
/// pub struct Shape {
///     x: Value<i32>,
///     label: Value<String>,
///     points: List<Value<(i32, i32)>>,
/// }
/// ```
///
/// `#[lattice(crate = "path")]` on the struct names `lattice-state` when it is not
/// `::lattice_state`.
#[proc_macro_derive(Synced, attributes(lattice))]
pub fn derive_synced(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    synced(&input)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

/// The path of `lattice-state` in generated code: `::lattice_state`, or the
/// `#[lattice(crate = "…")]` of the struct.
fn crate_path(input: &DeriveInput) -> Result<Tokens, Error> {
    let mut path = quote! { ::lattice_state };
    for attr in input.attrs.iter().filter(|a| a.path().is_ident("lattice")) {
        attr.parse_nested_meta(|meta| {
            if !meta.path.is_ident("crate") {
                return Err(meta.error("expected `crate = \"path\"`"));
            }
            let parsed: syn::Path = meta.value()?.parse::<syn::LitStr>()?.parse()?;
            path = quote! { #parsed };
            Ok(())
        })?;
    }
    Ok(path)
}

/// The parts of the input the generated code needs.
struct Input<'a> {
    name: &'a Ident,
    vis: &'a Visibility,
    idents: Vec<&'a Ident>,
    types: Vec<&'a Type>,
    /// Field names without `r#`, as serde names them.
    names: Vec<String>,
    krate: Tokens,
}

/// Checks the input shape: a non-generic struct with named fields.
fn parse(input: &DeriveInput) -> Result<Input<'_>, Error> {
    let Data::Struct(data) = &input.data else {
        return Err(Error::new(input.span(), "Synced: expected a struct"));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(Error::new(input.span(), "Synced: expected named fields"));
    };
    if !input.generics.params.is_empty() {
        return Err(Error::new(
            input.generics.span(),
            "Synced: generic structs are not supported",
        ));
    }
    let idents: Vec<_> = fields
        .named
        .iter()
        .filter_map(|f| f.ident.as_ref())
        .collect();
    let names = idents
        .iter()
        .map(|i| i.to_string().trim_start_matches("r#").to_owned())
        .collect();
    Ok(Input {
        name: &input.ident,
        vis: &input.vis,
        types: fields.named.iter().map(|f| &f.ty).collect(),
        idents,
        names,
        krate: crate_path(input)?,
    })
}

fn synced(input: &DeriveInput) -> Result<Tokens, Error> {
    let input = parse(input)?;
    let change = change_struct(&input);
    let imp = synced_impl(&input);
    Ok(quote! { #change #imp })
}

/// The `<Name>Change` struct: one `Option` per field, every field always encoded.
fn change_struct(input: &Input<'_>) -> Tokens {
    let Input {
        name,
        vis,
        idents,
        types,
        names,
        krate,
    } = input;
    let change = format_ident!("{}Change", name);
    let serde = quote! { #krate::__private::serde };
    let serde_path = serde.to_string().replace(' ', "");
    let field_docs: Vec<_> = names
        .iter()
        .map(|n| format!("Change of `{n}`, if any."))
        .collect();
    let change_doc = format!("A committed change of [`{name}`]: one optional change per field.");
    quote! {
        #[doc = #change_doc]
        #[derive(#serde::Serialize, #serde::Deserialize)]
        #[serde(crate = #serde_path)]
        #vis struct #change {
            #(
                #[doc = #field_docs]
                #vis #idents: ::std::option::Option<<#types as #krate::Synced>::Change>,
            )*
        }
    }
}

/// `impl Synced for <Name>`: each method asks every field.
fn synced_impl(input: &Input<'_>) -> Tokens {
    let Input {
        name,
        idents,
        names,
        krate,
        ..
    } = input;
    let change = format_ident!("{}Change", name);
    let change_name = change.to_string();
    let synced = quote! { #krate::Synced };
    let serde = quote! { #krate::__private::serde };
    let len = idents.len();
    quote! {
        impl #synced for #name {
            type Change = #change;

            fn has_draft(&self) -> bool {
                false #( || #synced::has_draft(&self.#idents) )*
            }

            fn write_change<S: #serde::Serializer>(&self, s: S) -> ::std::result::Result<S::Ok, S::Error> {
                use #serde::ser::SerializeStruct as _;
                let mut out = s.serialize_struct(#change_name, #len)?;
                #(
                    // Always written: a non-self-describing format (postcard) cannot skip a field.
                    let field = #synced::has_draft(&self.#idents).then_some(#krate::Draft(&self.#idents));
                    out.serialize_field(#names, &field)?;
                )*
                out.end()
            }

            fn commit(&mut self) {
                #( #synced::commit(&mut self.#idents); )*
            }

            fn rollback(&mut self) {
                #( #synced::rollback(&mut self.#idents); )*
            }

            fn apply(&mut self, change: #change) -> ::std::result::Result<(), #krate::ApplyError> {
                #(
                    if let ::std::option::Option::Some(change) = change.#idents {
                        #synced::apply(&mut self.#idents, change)?;
                    }
                )*
                ::std::result::Result::Ok(())
            }

            fn rev(&self) -> u64 {
                0u64 #( .saturating_add(#synced::rev(&self.#idents)) )*
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expand(input: Tokens) -> Result<String, Error> {
        let input: DeriveInput = syn::parse2(input)?;
        synced(&input).map(|t| t.to_string())
    }

    fn error(input: Tokens) -> String {
        match expand(input) {
            Ok(out) => format!("expanded: {out}"),
            Err(e) => e.to_string(),
        }
    }

    #[test]
    fn expands_change_struct_and_impl_with_default_crate_path() -> Result<(), Error> {
        let out = expand(quote! { pub struct Shape { x: Value<i32>, r#type: Value<u8> } })?;
        assert!(out.contains("pub struct ShapeChange"), "{out}");
        assert!(
            out.contains(":: lattice_state :: Synced for Shape"),
            "{out}"
        );
        assert!(out.contains("\"type\""), "raw ident name: {out}");
        assert!(!out.contains("skip"), "no skipped fields: {out}");
        Ok(())
    }

    #[test]
    fn crate_attribute_overrides_the_path() -> Result<(), Error> {
        let out = expand(quote! {
            #[lattice(crate = "my::state")]
            struct S { x: Value<i32> }
        })?;
        assert!(out.contains("my :: state :: Synced for S"), "{out}");
        assert!(!out.contains("lattice_state"), "{out}");
        Ok(())
    }

    #[test]
    fn refuses_bad_shapes() {
        let cases = [
            ("enum", quote! { enum E { A } }, "expected a struct"),
            ("tuple", quote! { struct T(u8); }, "expected named fields"),
            (
                "generic",
                quote! { struct G<T> { x: T } },
                "generic structs are not supported",
            ),
            (
                "attr",
                quote! { #[lattice(core = "x")] struct A { x: u8 } },
                "expected `crate = \"path\"`",
            ),
        ];
        for (name, input, want) in cases {
            let got = error(input);
            assert!(got.contains(want), "case={name}: {got}");
        }
    }
}
