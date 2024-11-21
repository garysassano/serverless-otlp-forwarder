use proc_macro::TokenStream;
use darling::{Error, FromMeta};
use darling::ast::NestedMeta;
use quote::{quote, format_ident};
use syn::ItemFn;
use syn::spanned::Spanned;

#[derive(Debug, FromMeta)]
struct RouteArgs {
    path: String,
    #[darling(default = "default_method")]
    method: String,
    #[darling(default = "default_module_name")]
    module: String,
    #[darling(default)]
    set_span_name: Option<bool>,
}

fn default_method() -> String {
    "GET".to_string()
}

fn default_module_name() -> String {
    "__lambda_lw_http_router_core_default_router".to_string()
}

/// Defines a route handler for Lambda HTTP events.
/// 
/// This attribute macro registers a function as a route handler in the router registry.
/// The function will be called when an incoming request matches the specified path and method.
/// Route handlers are registered at compile time, ensuring zero runtime overhead for route setup.
/// 
/// # Arguments
/// 
/// * `path` - The URL path to match (required). Supports path parameters like `{param_name}`
/// * `method` - The HTTP method to match (optional, defaults to "GET")
/// * `module` - The router module name (optional, defaults to internal name)
/// 
/// # Function Signature
/// 
/// The handler function must have exactly one parameter of type RouteContext:
/// 
/// ```rust,ignore
/// #[route(path = "/hello")]
/// async fn handle_hello(ctx: RouteContext) -> Result<Value, Error> {
///     Ok(json!({ "message": "Hello, World!" }))
/// }
/// ```
/// 
/// # Path Parameters
/// 
/// Path parameters are defined using curly braces and are available in the `RouteContext.params`:
/// * `/users/{id}` - Matches `/users/123` and provides `id = "123"`
/// * `/posts/{category}/{slug}` - Matches `/posts/tech/my-post`
/// 
/// # Examples
/// 
/// Route with path parameters and custom method:
/// ```rust,ignore
/// use lambda_lw_http_router::{route, define_router};
/// use aws_lambda_events::apigw::ApiGatewayV2httpRequest;
/// use serde_json::{json, Value};
/// use lambda_runtime::Error;
/// 
/// #[derive(Clone)]
/// struct AppState {
///     // your state fields here
/// }
/// 
/// define_router!(event = ApiGatewayV2httpRequest, state = AppState);
/// 
/// #[route(path = "/users/{id}", method = "POST", state = AppState)]
/// async fn create_user(ctx: RouteContext) -> Result<Value, Error> {
///     let user_id = ctx.params.get("id").unwrap();
///     Ok(json!({ "created": user_id }))
/// }
/// ```
#[proc_macro_attribute]
pub fn route(args: TokenStream, input: TokenStream) -> TokenStream {
    let attr_args = match NestedMeta::parse_meta_list(args.into()) {
        Ok(v) => v,
        Err(e) => { return TokenStream::from(Error::from(e).write_errors()); }
    };
    let input = syn::parse_macro_input!(input as ItemFn);
    
    let output: TokenStream = impl_router(attr_args, input).into();
    output
}

fn impl_router(args: Vec<NestedMeta>, input: ItemFn) -> proc_macro2::TokenStream {
    let route_args = match RouteArgs::from_list(&args) {
        Ok(v) => v,
        Err(e) => { return TokenStream::from(e.write_errors()).into(); }
    };

    let fn_name = &input.sig.ident;
    let method = &route_args.method;
    let path = &route_args.path;
    let module = format_ident!("{}", route_args.module);
    let register_fn = format_ident!("__register_{}", fn_name);

    // Validate function signature
    if input.sig.inputs.len() != 1 {
        return syn::Error::new(
            input.sig.span(),
            "Route handler must have exactly one parameter of type RouteContext"
        ).to_compile_error();
    }

    // Extract and validate the parameter type
    let param = input.sig.inputs.first().unwrap();
    match param {
        syn::FnArg::Typed(pat_type) => {
            match &*pat_type.ty {
                syn::Type::Path(type_path) => {
                    let last_segment = type_path.path.segments.last()
                        .ok_or_else(|| syn::Error::new(
                            type_path.span(),
                            "Invalid parameter type"
                        )).unwrap();
                    
                    if last_segment.ident != "RouteContext" {
                        return syn::Error::new(
                            type_path.span(),
                            "Parameter must be of type RouteContext"
                        ).to_compile_error();
                    }
                }
                _ => {
                    return syn::Error::new(
                        pat_type.ty.span(),
                        "Parameter must be of type RouteContext"
                    ).to_compile_error();
                }
            }
        }
        _ => {
            return syn::Error::new(
                param.span(),
                "Invalid parameter declaration"
            ).to_compile_error();
        }
    }

    // Default to true if not specified and otel feature is enabled
    let set_span_name = route_args.set_span_name.unwrap_or(true);
    let span_setting = if set_span_name {
        quote! {
            ctx.set_otel_span_name();
        }
    } else {
        quote! {}
    };

    let output = quote! {
        #[::lambda_lw_http_router::ctor::ctor]
        fn #register_fn() {
            ::lambda_lw_http_router::register_route::<#module::State, #module::Event>(
                #method,
                #path,
                |ctx| Box::pin(async move {
                    #span_setting
                    #fn_name(ctx).await
                })
            );
        }

        #input
    };
    output
}
