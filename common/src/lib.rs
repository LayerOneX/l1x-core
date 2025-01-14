use proc_macro::TokenStream;
use quote::{quote, quote_spanned};
use syn::{parse_macro_input, spanned::Spanned, DeriveInput};

#[proc_macro_derive(EnumDisplay)]
pub fn enum_display(input: TokenStream) -> TokenStream {
	// Parse the input tokens into a syntax tree
	let input = parse_macro_input!(input as DeriveInput);

	// Ensure the derive macro is used on an enum
	if let syn::Data::Enum(data_enum) = input.data {
		// Get the name of the enum
		let ident = &input.ident;

		// Collect the match arms for each variant
		let match_arms = data_enum.variants.iter().map(|variant| {
			let variant_name = &variant.ident;
			let doc_string = variant
				.attrs
				.iter()
				.find_map(|attr| {
					if let Ok(syn::Meta::NameValue(name_value)) = attr.parse_meta() {
						if name_value.path.is_ident("doc") {
							if let syn::Lit::Str(lit_str) = name_value.lit {
								return Some(lit_str.value())
							}
						}
					}
					None
				})
				.unwrap_or_else(|| "".to_string());
			quote_spanned! {variant.span()=>
				Self::#variant_name => {
					write!(f, "{}", #doc_string)?;
					write!(f, " after calling {:?}", Self::#variant_name)
				},
			}
		});

		// Generate the implementation for fmt::Display
		let expanded = quote! {
			impl ::std::fmt::Display for #ident {
				fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
					match self {
						#(#match_arms)*
					}
				}
			}
		};

		// Return the generated implementation as a TokenStream
		TokenStream::from(expanded)
	} else {
		// The derive macro is only applicable to enums
		let error_msg = "This derive macro only supports enums";
		TokenStream::from(quote_spanned! {
			input.span()=>
			compile_error!(#error_msg);
		})
	}
}
