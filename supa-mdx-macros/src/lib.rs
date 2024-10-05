use proc_macro::TokenStream;
use quote::quote;

#[proc_macro_derive(RuleName)]
pub fn rule_name_derive(input: TokenStream) -> TokenStream {
    let ast = syn::parse(input).unwrap();
    impl_rule_name_macro(&ast)
}

fn impl_rule_name_macro(ast: &syn::DeriveInput) -> TokenStream {
    let name = &ast.ident;
    let gen = quote! {
        impl RuleName for #name {
            fn name(&self) -> &'static str {
                stringify!(#name)
            }
        }
    };
    gen.into()
}
