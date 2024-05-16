use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use syn::{parse_macro_input, FnArg, ItemFn};

struct Args {
   should_fail: bool
}

fn parse_args(attr_args: syn::AttributeArgs) -> syn::Result<Args> {
    let mut should_fail = false;
    for arg in attr_args {
        match arg {
            syn::NestedMeta::Meta(syn::Meta::NameValue(namevalue))
                if namevalue.path.is_ident("should_fail") => {
                    should_fail = match namevalue.lit {
                        syn::Lit::Bool(litbool) => litbool.value,
                        _ => {
                            return Err(syn::Error::new_spanned(
                                namevalue,
                                "expected `true` or `false`",
                            ))
                        }
                    };
                },
            other => {
                return Err(syn::Error::new_spanned(
                    other,
                    "expected `should_fail = true | false`",
                ))
            }

        }
    }
    Ok(Args { should_fail })
}

#[proc_macro_attribute]
pub fn test(args: TokenStream, item: TokenStream) -> TokenStream {
    let args = syn::parse_macro_input!(args as syn::AttributeArgs);
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
    let should_fail = if parse_args(args).unwrap().should_fail {
        quote! {
            #[should_panic]
        }
    } else {
        quote! { }
    };

    quote! {
        #[sqlx::test]
        #should_fail
        async fn #function_name(#arguments) #return_type {
            #migrate
            #block
        }
    }.into()
}
