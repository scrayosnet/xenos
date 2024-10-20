use darling::ast::NestedMeta;
use darling::util::IdentString;
use darling::{Error, FromMeta};
use proc_macro::TokenStream;
use quote::quote;
use std::collections::HashMap;

#[derive(Debug, FromMeta)]
struct MetricsMacroArgs {
    #[darling(default)]
    metric: String,
    #[darling(default)]
    labels: Option<HashMap<String, String>>,
    handler: IdentString,
}

#[proc_macro_attribute]
pub fn metrics(args: TokenStream, input: TokenStream) -> TokenStream {
    metrics_impl(args.into(), input.into()).into()
}

fn metrics_impl(
    args: proc_macro2::TokenStream,
    input: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    // parse input struct and handle parse errors
    let input_fn: syn::ItemFn = match syn::parse2::<syn::ItemFn>(input.clone()) {
        Ok(is) => is,
        Err(e) => {
            return Error::from(e).write_errors();
        }
    };

    // parse attributes using darling and handle errors
    let attr_args = match NestedMeta::parse_meta_list(args) {
        Ok(v) => v,
        Err(e) => return Error::from(e).write_errors(),
    };
    let args = match MetricsMacroArgs::from_list(&attr_args) {
        Ok(v) => v,
        Err(e) => {
            return e.write_errors();
        }
    };

    let fn_head = &input_fn.sig;
    let fn_vis = &input_fn.vis;
    let fn_block = &input_fn.block;
    let metric = args.metric;
    let labels = args.labels.unwrap_or_else(HashMap::new);
    let handler = args.handler;

    let inner_fn = match fn_head.asyncness {
        Some(_) => quote! {
            (|| async move { #fn_block })().await
        },
        None => quote! {
            (move || { #fn_block })()
        },
    };

    let label_keys = labels.keys();
    let label_values = labels.values();
    let result = quote! {
        #fn_vis #fn_head {
            let start = ::std::time::Instant::now();
            let result = #inner_fn;

            #handler(::metrics::MetricsEvent{
                metric: #metric,
                labels: ::metrics::HashMap::from([
                    #((#label_keys, #label_values),)*
                ]),
                time: start.elapsed().as_secs_f64(),
                result: &result,
            });
            result
        }
    };

    result
}
