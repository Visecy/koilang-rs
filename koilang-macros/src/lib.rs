use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{
    parse_macro_input, Attribute, Expr, ExprLit, ImplItem, ItemFn, ItemImpl, Lit, Meta, MetaNameValue,
    Pat, PatType, Type, TypeReference,
};

/// The crate name to use for koilang types.
/// When the macro is used within koilang-rs itself, we use `crate::`.
/// Otherwise, we use `::koilang::`.
fn koilang_crate() -> proc_macro2::TokenStream {
    // Check if we're compiling within the koilang-rs crate itself
    // by checking the CARGO_PKG_NAME environment variable
    let pkg_name = std::env::var("CARGO_PKG_NAME").unwrap_or_default();
    if pkg_name == "koilang" {
        quote!(crate)
    } else {
        quote!(::koilang)
    }
}

/// Parse the command name from the attribute.
///
/// Supports:
/// - `#[command]` - uses function name as command name
/// - `#[command(name = "custom_name")]` - uses specified name
fn parse_command_name(attrs: &[Attribute], default_name: &str) -> syn::Result<String> {
    for attr in attrs {
        if attr.path().is_ident("command") {
            // Check if there are any arguments
            let meta_result = attr.parse_args::<Meta>();
            match meta_result {
                Ok(meta) => {
                    if let Meta::NameValue(MetaNameValue { path, value, .. }) = meta {
                        if path.is_ident("name") {
                            if let Expr::Lit(ExprLit {
                                lit: Lit::Str(lit_str),
                                ..
                            }) = value
                            {
                                return Ok(lit_str.value());
                            } else {
                                return Err(syn::Error::new(
                                    Span::call_site(),
                                    "expected string literal for 'name'",
                                ));
                            }
                        }
                    }
                }
                Err(_) => {
                    // No arguments provided, use default name
                    return Ok(default_name.to_string());
                }
            }
        }
    }
    Ok(default_name.to_string())
}

/// Check if a function has the `#[command]` attribute.
fn has_command_attr(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident("command"))
}

/// Check if a type is `&mut Runtime`.
///
/// This detects both:
/// - `&mut Runtime` (simple path)
/// - `&mut koilang::Runtime` or `&mut ::koilang::Runtime` (qualified path)
fn is_runtime_type(ty: &Type) -> bool {
    if let Type::Reference(TypeReference {
        mutability: Some(_),
        elem,
        ..
    }) = ty
    {
        if let Type::Path(type_path) = &**elem {
            // Check if the last segment is "Runtime"
            if let Some(segment) = type_path.path.segments.last() {
                return segment.ident == "Runtime";
            }
        }
    }
    false
}

/// Generate argument extraction code for a function parameter.
///
/// This generates code that converts a `Value` to the appropriate Rust type.
/// Returns `Ok(None)` for Runtime parameters (they are injected, not extracted).
fn generate_arg_extraction(
    pat: &Pat,
    ty: &Type,
    index: usize,
) -> syn::Result<Option<proc_macro2::TokenStream>> {
    // Skip Runtime parameters - they are injected, not extracted from args
    if is_runtime_type(ty) {
        return Ok(None);
    }

    let var_name = match pat {
        Pat::Ident(pat_ident) => &pat_ident.ident,
        _ => {
            return Err(syn::Error::new(
                Span::call_site(),
                "only simple identifiers are supported as parameter names",
            ))
        }
    };

    // Generate extraction code based on type
    // Value is an enum with variants: Int(i64), Float(f64), Bool(bool), String(String)
    let extraction = match ty {
        // String type
        Type::Path(type_path) if type_path.path.is_ident("String") => {
            quote! {
                let #var_name = args.get(#index)
                    .map(|v| match v {
                        koicore::command::Value::String(s) => s.clone(),
                        koicore::command::Value::Int(i) => i.to_string(),
                        koicore::command::Value::Float(f) => f.to_string(),
                        koicore::command::Value::Bool(b) => b.to_string(),
                    })
                    .unwrap_or_default();
            }
        }
        // &str type
        Type::Reference(type_ref) => {
            if let Type::Path(inner_path) = &*type_ref.elem {
                if inner_path.path.is_ident("str") {
                    quote! {
                        let #var_name: &str = args.get(#index)
                            .map(|v| match v {
                                koicore::command::Value::String(s) => s.as_str(),
                                koicore::command::Value::Int(i) => {
                                    // This is a limitation - we can't return a reference to a temporary
                                    // For &str parameters, we'll need to handle this differently
                                    // For now, use empty string as fallback
                                    ""
                                }
                                koicore::command::Value::Float(_) => "",
                                koicore::command::Value::Bool(_) => "",
                            })
                            .unwrap_or("");
                    }
                } else {
                    return Err(syn::Error::new(
                        Span::call_site(),
                        format!("unsupported reference type for parameter '{}'", var_name),
                    ));
                }
            } else {
                return Err(syn::Error::new(
                    Span::call_site(),
                    format!("unsupported reference type for parameter '{}'", var_name),
                ));
            }
        }
        // i32/i64 integer types
        Type::Path(type_path) => {
            let type_str = quote!(#type_path).to_string();
            match type_str.as_str() {
                "i32" | "i64" => {
                    quote! {
                        let #var_name: #type_path = args.get(#index)
                            .map(|v| match v {
                                koicore::command::Value::Int(i) => *i as #type_path,
                                koicore::command::Value::Float(f) => *f as #type_path,
                                koicore::command::Value::String(s) => s.parse().unwrap_or_default(),
                                koicore::command::Value::Bool(b) => if *b { 1 } else { 0 },
                            })
                            .unwrap_or_default();
                    }
                }
                "f32" | "f64" => {
                    quote! {
                        let #var_name: #type_path = args.get(#index)
                            .map(|v| match v {
                                koicore::command::Value::Float(f) => *f as #type_path,
                                koicore::command::Value::Int(i) => *i as #type_path,
                                koicore::command::Value::String(s) => s.parse().unwrap_or_default(),
                                koicore::command::Value::Bool(b) => if *b { 1.0 } else { 0.0 },
                            })
                            .unwrap_or_default();
                    }
                }
                "bool" => {
                    quote! {
                        let #var_name: bool = args.get(#index)
                            .map(|v| match v {
                                koicore::command::Value::Bool(b) => *b,
                                koicore::command::Value::Int(i) => *i != 0,
                                koicore::command::Value::Float(f) => *f != 0.0,
                                koicore::command::Value::String(s) => !s.is_empty(),
                            })
                            .unwrap_or_default();
                    }
                }
                _ => {
                    return Err(syn::Error::new(
                        Span::call_site(),
                        format!(
                            "unsupported type '{}' for parameter '{}'",
                            type_str, var_name
                        ),
                    ));
                }
            }
        }
        _ => {
            return Err(syn::Error::new(
                Span::call_site(),
                format!("unsupported type for parameter '{}'", var_name),
            ));
        }
    };

    Ok(Some(extraction))
}

/// Attribute macro for marking a function as a command.
///
/// This macro is used to annotate methods that represent KoiLang commands.
/// It can be used with or without arguments:
///
/// - `#[command]` - uses the function name as the command name
/// - `#[command(name = "custom_name")]` - uses the specified command name
///
/// # Examples
///
/// ```rust
/// #[command]
/// fn greet(&mut self, name: String) { ... }
///
/// #[command(name = "@start")]
/// fn on_start(&mut self) { ... }
/// ```
#[proc_macro_attribute]
pub fn command(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse the attribute arguments
    let _attr = proc_macro2::TokenStream::from(attr);

    // Parse the input function
    let input_fn = parse_macro_input!(item as ItemFn);

    // For now, just pass through the function unchanged
    // The actual processing is done by #[command_handler]
    let expanded = quote! {
        #input_fn
    };

    TokenStream::from(expanded)
}

/// Attribute macro for generating a `CommandHandler` implementation.
///
/// This macro should be placed on an impl block. It will:
/// 1. Find all methods marked with `#[command]`
/// 2. Generate a `CommandHandler` trait implementation
/// 3. Create a `handle_command` method that dispatches to the marked methods
///
/// # Examples
///
/// ```rust,ignore
/// #[command_handler]
/// impl MyEnv {
///     #[command]
///     fn greet(&mut self, name: String) { ... }
///
///     #[command(name = "@start")]
///     fn on_start(&mut self) { ... }
/// }
/// ```
#[proc_macro_attribute]
pub fn command_handler(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse the impl block
    let input_impl = parse_macro_input!(item as ItemImpl);

    // Get the type being implemented
    let self_ty = &input_impl.self_ty;

    // Get the appropriate crate path
    let koi = koilang_crate();

    // Collect all methods marked with #[command]
    let mut command_methods = Vec::new();

    for item in &input_impl.items {
        if let ImplItem::Fn(method) = item {
            if has_command_attr(&method.attrs) {
                // Parse the command name from the attribute
                let method_name = method.sig.ident.to_string();
                let command_name = match parse_command_name(&method.attrs, &method_name) {
                    Ok(name) => name,
                    Err(e) => return e.to_compile_error().into(),
                };

                command_methods.push((command_name, method.clone()));
            }
        }
    }

    // Generate match arms for each command
    let mut match_arms = Vec::new();

    for (cmd_name, method) in command_methods {
        let method_ident = &method.sig.ident;
        let cmd_name_lit = cmd_name;

        // Analyze parameters and generate extraction/injection code
        // Skip &mut self (first parameter), then process remaining parameters
        let mut arg_extractions = Vec::new();
        let mut arg_expressions = Vec::new();
        let mut arg_index: usize = 0;

        for param in method.sig.inputs.iter().skip(1) {
            if let syn::FnArg::Typed(PatType { pat, ty, .. }) = param {
                // Check if this is a Runtime parameter
                if is_runtime_type(ty) {
                    // Inject runtime reference
                    arg_expressions.push(quote!(runtime));
                } else {
                    // Generate extraction code for regular argument
                    match generate_arg_extraction(pat, ty, arg_index) {
                        Ok(Some(extraction)) => {
                            arg_extractions.push(extraction);
                            // Get the variable name for the method call
                            if let Pat::Ident(pat_ident) = &**pat {
                                let var_name = &pat_ident.ident;
                                arg_expressions.push(quote!(#var_name));
                            }
                            arg_index += 1;
                        }
                        Ok(None) => {
                            // Runtime type - should have been caught above, but handle just in case
                            arg_expressions.push(quote!(runtime));
                        }
                        Err(e) => return e.to_compile_error().into(),
                    }
                }
            }
        }

        // Generate the match arm
        let match_arm = quote! {
            #cmd_name_lit => {
                #(#arg_extractions)*
                self.#method_ident(#(#arg_expressions),*);
                Ok(())
            }
        };

        match_arms.push(match_arm);
    }

    // Generate the full implementation
    let expanded = quote! {
        #input_impl

        impl #koi::CommandHandler for #self_ty {
            fn handle_command(
                &mut self,
                name: &str,
                args: &[#koi::Value],
                _kwargs: &::std::collections::HashMap<String, #koi::Value>,
                runtime: &mut #koi::Runtime,
            ) -> #koi::Result<()> {
                match name {
                    #(#match_arms)*
                    _ => Err(#koi::KoiError::command_not_found(name)),
                }
            }
        }
    };

    TokenStream::from(expanded)
}
