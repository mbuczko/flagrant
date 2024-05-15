use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use syn::{parse_macro_input, FnArg, ItemFn};

#[proc_macro_attribute]
pub fn test(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let function_name = input.sig.ident.clone();
    let arguments = input.sig.inputs;
    let return_type = input.sig.output.clone();

    let block = &input.block;
    let migrate = if let Some(FnArg::Typed(arg)) = arguments.first() {
        let name = arg.pat.clone().into_token_stream();
        quote! { flagrant::db::migrate(&#name, semver::Version::parse("0.0.1").unwrap()).await.unwrap(); }
    } else {
        quote! {}
    };

    quote! {
        #[sqlx::test]
        async fn #function_name(#arguments) #return_type {
            #migrate
            #block
        }
    }.into()
}
