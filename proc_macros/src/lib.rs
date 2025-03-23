extern crate proc_macro;
use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, Data, DeriveInput};

/// This procedural macro create default deserealization functions for each
/// structure attribute based on std::Default impl.
///
/// It applies default values only for fields missing from the deserialized input.
///
/// # Example
///
/// ```rust
/// use serde::Deserialize;
/// use your_proc_macro_crate::serde_individual_default;
///
/// #[derive(Deserialize, Getters)]
/// #[serde_individual_default]
/// struct Example {
///     #[getter(skip)]
///     test_1: i64,
///     test_2: i64,
///     test_3: String,
/// }
/// impl Default for Example {
///     fn default() -> Self {
///         Example {
///             test_1: 3942,
///             test_2: 42390,
///             test_3: "a".to_string(),
///         }
///     }
/// }
///
/// let json_data_1 = serde_json::json!({
///     "test_1": 500,
///     "test_2": 100
/// });
/// let example_struct_1: Example = serde_json::from_value(json_data_1).unwrap();
/// assert_eq!(example_struct_1.test_1, 500);
/// assert_eq!(example_struct_1.test_2, 100);
/// assert_eq!(example_struct_1.test_3, "a".to_string());
/// ```
#[proc_macro_attribute]
pub fn serde_individual_default(_attr: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let struct_name = &input.ident;
    let struct_generics = &input.generics;
    let struct_fields = match &input.data {
        Data::Struct(s) => &s.fields,
        _ => panic!("SerdeIndividualDefault can only be used with structs"),
    };
    let struct_attrs = &input.attrs;
    let struct_visibility = &input.vis;

    let struct_name_str = struct_name.to_string();
    let (struct_impl_generics, struct_ty_generics, struct_where_clause) =
        struct_generics.split_for_impl();

    // store only one struct::default object in memory
    let default_config_struct_name =
        format_ident!("DEFAULT_{}_STATIC", struct_name_str.to_ascii_uppercase());
    let struct_default_lazy_construction_definition = {
        quote! {
            lazy_static::lazy_static! {
                static ref #default_config_struct_name: #struct_name = #struct_name::default();
            }
        }
    };

    // build struct attributes with #[serde(default = "")] and build the default function itself
    let (all_field_attrs, default_deserialize_function_definitions) = struct_fields.iter().fold(
        (vec![], vec![]),
        |(mut all_field_attrs, mut default_deserialize_function_definitions), field| {
            let field_name = &field.ident;
            let field_type = &field.ty;
            let field_vis = &field.vis;
            let field_attrs = &field.attrs;
            let field_name_str = field_name.as_ref().unwrap().to_string();

            // default function name will be named default_{struct_name}_{field_name}
            let default_deserialize_function_name =
                format_ident!("default_{}_{}", struct_name_str, field_name_str);

            let default_deserialize_function_name_str =
                default_deserialize_function_name.to_string();

            all_field_attrs.push(quote! {
                #(#field_attrs)*
                #[serde(default = #default_deserialize_function_name_str)]
                #field_vis #field_name: #field_type,
            });
            default_deserialize_function_definitions.push(quote! {
                fn #default_deserialize_function_name() -> #field_type {
                    #default_config_struct_name.#field_name.clone()
                }
            });

            (all_field_attrs, default_deserialize_function_definitions)
        },
    );

    // build final struct.
    //We have to explicitly derive Deserialize here so the serde attribute works
    let expanded_token_stream = quote! {
        #[derive(serde::Deserialize)]
        #(#struct_attrs)*
        #struct_visibility struct #struct_name #struct_impl_generics {
            #(#all_field_attrs)*
        } #struct_ty_generics #struct_where_clause

        #struct_default_lazy_construction_definition

        #(#default_deserialize_function_definitions)*
    };
    TokenStream::from(expanded_token_stream)
}
