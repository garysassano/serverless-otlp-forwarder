use proc_macro::TokenStream;
use darling::{Error, FromMeta};
use darling::ast::NestedMeta;
use quote::{quote, format_ident};
use syn::ItemFn;
use lambda_lw_router_core::DEFAULT_ROUTER_MODULE;

#[derive(Debug, FromMeta)]
struct RouteArgs {
    path: String,
    #[darling(default = "default_method")]
    method: String,
    #[darling(default = "default_module_name")]
    module: String,
}

fn default_method() -> String {
    "GET".to_string()
}

fn default_module_name() -> String {
    DEFAULT_ROUTER_MODULE.to_string()
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
/// # Path Parameters
/// 
/// Path parameters are defined using curly braces and are available in the `RouteContext.params`:
/// * `/users/{id}` - Matches `/users/123` and provides `id = "123"`
/// * `/posts/{category}/{slug}` - Matches `/posts/tech/my-post`
/// 
/// # Examples
/// 
/// Basic GET route:
/// ```rust
/// #[route(path = "/hello")]
/// async fn handle_hello(ctx: RouteContext) -> Result<Value, Error> {
///     Ok(json!({ "message": "Hello, World!" }))
/// }
/// ```
/// 
/// Route with path parameters and custom method:
/// ```rust
/// #[route(path = "/users/{id}", method = "POST")]
/// async fn create_user(ctx: RouteContext) -> Result<Value, Error> {
///     let user_id = ctx.params.get("id").unwrap();
///     // ... handle user creation ...
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
    let fn_block = &input.block;
    let method = &route_args.method;
    let path = &route_args.path;
    let module = format_ident!("{}", route_args.module);
    let register_fn = format_ident!("__register_{}", fn_name);
    
    quote! {
        #[::lambda_lw_router::ctor::ctor]
        fn #register_fn() {
            ::lambda_lw_router::register_route::<AppState, #module::Event>(
                #method,
                #path,
                |ctx| Box::pin(#fn_name(ctx))
            );
        }

        async fn #fn_name(ctx: #module::RouteContext) -> Result<Value, Error> #fn_block
    }
}
