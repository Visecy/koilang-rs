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
    let pkg_name = std::env::var("CARGO_PKG_NAME").unwrap_or_default();
    if pkg_name == "koilang" {
        quote!(crate)
    } else {
        quote!(::koilang)
    }
}

/// Parsed options from the `#[command]` attribute.
struct CommandOptions {
    name: String,
    allow_int_to_float: bool,
}

/// Parse options from the `#[command]` attribute.
///
/// Supports:
/// - `#[command]` - uses function name as command name
/// - `#[command(name = "custom_name")]` - uses specified name
/// - `#[command(allow_int_to_float)]` - enables int-to-float conversion
/// - `#[command(name = "custom_name", allow_int_to_float)]` - both options
fn parse_command_options(attrs: &[Attribute], default_name: &str) -> syn::Result<CommandOptions> {
    for attr in attrs {
        if attr.path().is_ident("command") {
            let mut name = default_name.to_string();
            let mut allow_int_to_float = false;

            let meta_result = attr.parse_args::<Meta>();
            match meta_result {
                Ok(meta) => {
                    match meta {
                        Meta::Path(_) => {
                            // `#[command(allow_int_to_float)]` - single path
                            if let Some(segment) = meta.path().segments.last() {
                                if segment.ident == "allow_int_to_float" {
                                    allow_int_to_float = true;
                                }
                            }
                        }
                        Meta::NameValue(MetaNameValue { path, value, .. }) => {
                            // `#[command(name = "custom_name")]`
                            if path.is_ident("name") {
                                if let Expr::Lit(ExprLit {
                                    lit: Lit::Str(lit_str),
                                    ..
                                }) = value
                                {
                                    name = lit_str.value();
                                } else {
                                    return Err(syn::Error::new(
                                        Span::call_site(),
                                        "expected string literal for 'name'",
                                    ));
                                }
                            }
                        }
                        Meta::List(meta_list) => {
                            // `#[command(name = "custom_name", allow_int_to_float)]`
                            let nested: syn::punctuated::Punctuated<Meta, syn::Token![,]> =
                                meta_list.parse_args_with(syn::punctuated::Punctuated::parse_terminated)?;
                            
                            for nested_meta in nested {
                                match nested_meta {
                                    Meta::NameValue(MetaNameValue { path, value, .. }) => {
                                        if path.is_ident("name") {
                                            if let Expr::Lit(ExprLit {
                                                lit: Lit::Str(lit_str),
                                                ..
                                            }) = value
                                            {
                                                name = lit_str.value();
                                            } else {
                                                return Err(syn::Error::new(
                                                    Span::call_site(),
                                                    "expected string literal for 'name'",
                                                ));
                                            }
                                        }
                                    }
                                    Meta::Path(path) => {
                                        if path.is_ident("allow_int_to_float") {
                                            allow_int_to_float = true;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                Err(_) => {
                    // No arguments provided, use defaults
                    return Ok(CommandOptions {
                        name: default_name.to_string(),
                        allow_int_to_float: false,
                    });
                }
            }

            return Ok(CommandOptions { name, allow_int_to_float });
        }
    }
    Ok(CommandOptions {
        name: default_name.to_string(),
        allow_int_to_float: false,
    })
}

/// Check if a function has the `#[command]` attribute.
fn has_command_attr(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident("command"))
}

/// Check if a type is `&mut Runtime`.
fn is_runtime_type(ty: &Type) -> bool {
    if let Type::Reference(TypeReference {
        mutability: Some(_),
        elem,
        ..
    }) = ty
    {
        if let Type::Path(type_path) = &**elem {
            if let Some(segment) = type_path.path.segments.last() {
                return segment.ident == "Runtime";
            }
        }
    }
    false
}

/// Generate argument extraction code for a function parameter.
///
/// This generates code that converts a `Value` to the appropriate Rust type
/// with strict type checking. Returns a runtime error on type mismatch.
/// Returns `Ok(None)` for Runtime parameters (they are injected, not extracted).
fn generate_arg_extraction(
    pat: &Pat,
    ty: &Type,
    index: usize,
    allow_int_to_float: bool,
    koi: &proc_macro2::TokenStream,
) -> syn::Result<Option<proc_macro2::TokenStream>> {
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

    let extraction = match ty {
        Type::Path(type_path) if type_path.path.is_ident("String") => {
            quote! {
                let #var_name: String = match args.get(#index) {
                    Some(koicore::command::Value::String(s)) => s.clone(),
                    Some(other) => {
                        let type_name = match other {
                            koicore::command::Value::Int(_) => "Int",
                            koicore::command::Value::Float(_) => "Float",
                            koicore::command::Value::String(_) => "String",
                            koicore::command::Value::Bool(_) => "Bool",
                        };
                        return Err(#koi::KoiError::runtime(
                            format!("type mismatch for argument {}: expected String, got {}", #index, type_name)
                        ));
                    }
                    None => String::new(),
                };
            }
        }
        Type::Reference(type_ref) => {
            if let Type::Path(inner_path) = &*type_ref.elem {
                if inner_path.path.is_ident("str") {
                    quote! {
                        let #var_name: &str = match args.get(#index) {
                            Some(koicore::command::Value::String(s)) => s.as_str(),
                            Some(other) => {
                                let type_name = match other {
                                    koicore::command::Value::Int(_) => "Int",
                                    koicore::command::Value::Float(_) => "Float",
                                    koicore::command::Value::String(_) => "String",
                                    koicore::command::Value::Bool(_) => "Bool",
                                };
                                return Err(#koi::KoiError::runtime(
                                    format!("type mismatch for argument {}: expected String, got {}", #index, type_name)
                                ));
                            }
                            None => "",
                        };
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
        Type::Path(type_path) => {
            let type_str = quote!(#type_path).to_string();
            match type_str.as_str() {
                "i32" | "i64" => {
                    quote! {
                        let #var_name: #type_path = match args.get(#index) {
                            Some(koicore::command::Value::Int(i)) => *i as #type_path,
                            Some(other) => {
                                let type_name = match other {
                                    koicore::command::Value::Int(_) => "Int",
                                    koicore::command::Value::Float(_) => "Float",
                                    koicore::command::Value::String(_) => "String",
                                    koicore::command::Value::Bool(_) => "Bool",
                                };
                                return Err(#koi::KoiError::runtime(
                                    format!("type mismatch for argument {}: expected Int, got {}", #index, type_name)
                                ));
                            }
                            None => 0 as #type_path,
                        };
                    }
                }
                "f32" | "f64" => {
                    if allow_int_to_float {
                        quote! {
                            let #var_name: #type_path = match args.get(#index) {
                                Some(koicore::command::Value::Float(f)) => *f as #type_path,
                                Some(koicore::command::Value::Int(i)) => *i as #type_path,
                                Some(other) => {
                                    let type_name = match other {
                                        koicore::command::Value::Int(_) => "Int",
                                        koicore::command::Value::Float(_) => "Float",
                                        koicore::command::Value::String(_) => "String",
                                        koicore::command::Value::Bool(_) => "Bool",
                                    };
                                    return Err(#koi::KoiError::runtime(
                                        format!("type mismatch for argument {}: expected Float, got {}", #index, type_name)
                                    ));
                                }
                                None => 0.0 as #type_path,
                            };
                        }
                    } else {
                        quote! {
                            let #var_name: #type_path = match args.get(#index) {
                                Some(koicore::command::Value::Float(f)) => *f as #type_path,
                                Some(other) => {
                                    let type_name = match other {
                                        koicore::command::Value::Int(_) => "Int",
                                        koicore::command::Value::Float(_) => "Float",
                                        koicore::command::Value::String(_) => "String",
                                        koicore::command::Value::Bool(_) => "Bool",
                                    };
                                    return Err(#koi::KoiError::runtime(
                                        format!("type mismatch for argument {}: expected Float, got {}", #index, type_name)
                                    ));
                                }
                                None => 0.0 as #type_path,
                            };
                        }
                    }
                }
                "bool" => {
                    quote! {
                        let #var_name: bool = match args.get(#index) {
                            Some(koicore::command::Value::Bool(b)) => *b,
                            Some(other) => {
                                let type_name = match other {
                                    koicore::command::Value::Int(_) => "Int",
                                    koicore::command::Value::Float(_) => "Float",
                                    koicore::command::Value::String(_) => "String",
                                    koicore::command::Value::Bool(_) => "Bool",
                                };
                                return Err(#koi::KoiError::runtime(
                                    format!("type mismatch for argument {}: expected Bool, got {}", #index, type_name)
                                ));
                            }
                            None => false,
                        };
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
/// It can be used with various options:
///
/// - `#[command]` - uses the function name as the command name
/// - `#[command(name = "custom_name")]` - uses the specified command name
/// - `#[command(allow_int_to_float)]` - enables int-to-float conversion for float params
/// - `#[command(name = "custom_name", allow_int_to_float)]` - both options
///
/// # Examples
///
/// ```rust
/// #[command]
/// fn greet(&mut self, name: String) { ... }
///
/// #[command(name = "@start")]
/// fn on_start(&mut self) { ... }
///
/// #[command(allow_int_to_float)]
/// fn take_number(&mut self, value: f64) { ... }
/// ```
#[proc_macro_attribute]
pub fn command(attr: TokenStream, item: TokenStream) -> TokenStream {
    let _attr = proc_macro2::TokenStream::from(attr);
    let input_fn = parse_macro_input!(item as ItemFn);
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
///
///     #[command(allow_int_to_float)]
///     fn take_number(&mut self, value: f64) { ... }
/// }
/// ```
#[proc_macro_attribute]
pub fn command_handler(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_impl = parse_macro_input!(item as ItemImpl);
    let self_ty = &input_impl.self_ty;
    let koi = koilang_crate();

    let mut command_methods = Vec::new();

    for item in &input_impl.items {
        if let ImplItem::Fn(method) = item {
            if has_command_attr(&method.attrs) {
                let method_name = method.sig.ident.to_string();
                let options = match parse_command_options(&method.attrs, &method_name) {
                    Ok(opts) => opts,
                    Err(e) => return e.to_compile_error().into(),
                };
                command_methods.push((options.name, method.clone(), options.allow_int_to_float));
            }
        }
    }

    let mut match_arms = Vec::new();

    for (cmd_name, method, allow_int_to_float) in command_methods {
        let method_ident = &method.sig.ident;
        let cmd_name_lit = cmd_name;

        let mut arg_extractions = Vec::new();
        let mut arg_expressions = Vec::new();
        let mut arg_index: usize = 0;

        for param in method.sig.inputs.iter().skip(1) {
            if let syn::FnArg::Typed(PatType { pat, ty, .. }) = param {
                if is_runtime_type(ty) {
                    arg_expressions.push(quote!(runtime));
                } else {
                    match generate_arg_extraction(pat, ty, arg_index, allow_int_to_float, &koi) {
                        Ok(Some(extraction)) => {
                            arg_extractions.push(extraction);
                            if let Pat::Ident(pat_ident) = &**pat {
                                let var_name = &pat_ident.ident;
                                arg_expressions.push(quote!(#var_name));
                            }
                            arg_index += 1;
                        }
                        Ok(None) => {
                            arg_expressions.push(quote!(runtime));
                        }
                        Err(e) => return e.to_compile_error().into(),
                    }
                }
            }
        }

        let match_arm = quote! {
            #cmd_name_lit => {
                #(#arg_extractions)*
                self.#method_ident(#(#arg_expressions),*);
                Ok(())
            }
        };

        match_arms.push(match_arm);
    }

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
