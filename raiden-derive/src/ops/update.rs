use quote::*;

pub(crate) fn expand_update_item(
    partition_key: &proc_macro2::Ident,
    sort_key: &Option<proc_macro2::Ident>,
    attr_enum_name: &proc_macro2::Ident,
    struct_name: &proc_macro2::Ident,
    // fields: &syn::FieldsNamed,
) -> proc_macro2::TokenStream {
    let item_output_name = format_ident!("{}UpdateItemOutput", struct_name);
    let trait_name = format_ident!("{}UpdateItem", struct_name);
    let client_name = format_ident!("{}Client", struct_name);
    let builder_name = format_ident!("{}UpdateItemBuilder", struct_name);

    // let condition_token_name = format_ident!("{}ConditionToken", struct_name);

    quote! {
        #[derive(Debug, Clone, PartialEq)]
        pub struct #item_output_name {
            // #(#output_fields)*
        }

        pub trait #trait_name {
            fn update(&self, key: impl ::raiden::IntoAttribute + std::marker::Send) -> #builder_name;
        }

        impl #trait_name for #client_name {
            fn update(&self, key: impl ::raiden::IntoAttribute + std::marker::Send) -> #builder_name{
                let mut input = ::raiden::UpdateItemInput::default();
                let key_attr: AttributeValue = key.into_attr();
                let mut key: std::collections::HashMap<String, AttributeValue> = std::collections::HashMap::new();
                key.insert(stringify!(#partition_key).to_owned(), key_attr);
                input.key = key;
                input.table_name = self.table_name();
                #builder_name {
                    client: &self.client,
                    input,
                    set_items: vec![],
                    add_items: vec![],
                    remove_items: vec![],
                    delete_items: vec![],
                }
            }
        }

        pub struct #builder_name<'a> {
            pub client: &'a ::raiden::DynamoDbClient,
            pub input: ::raiden::UpdateItemInput,
            pub add_items: Vec<(#attr_enum_name, ::raiden::AttributeValue)>,
            pub set_items: Vec<(#attr_enum_name, ::raiden::AttributeValue)>,
            pub remove_items: Vec<#attr_enum_name>,
            pub delete_items: Vec<(#attr_enum_name, ::raiden::AttributeValue)>,
        }

        impl<'a> #builder_name<'a> {

            pub fn raw_input(mut self, input: ::raiden::UpdateItemInput) -> Self {
                self.input = input;
                self
            }

            pub fn add(mut self, attr: #attr_enum_name, value: impl ::raiden::IntoAttribute) -> Self {
                self.add_items.push((attr, value.into_attr()));
                self
            }

            pub fn set(mut self, attr: #attr_enum_name, value: impl ::raiden::IntoAttribute) -> Self {
                self.set_items.push((attr, value.into_attr()));
                self
            }

            pub fn remove(mut self, attr: #attr_enum_name) -> Self {
                self.remove_items.push(attr);
                self
            }

            pub fn delete(mut self, attr: #attr_enum_name, value: impl ::raiden::IntoAttribute) -> Self {
                self.delete_items.push((attr, value.into_attr()));
                self
            }

            pub fn sort_key(mut self, key: impl IntoAttribute + std::marker::Send) -> Self {
                let key_attr: AttributeValue = key.into_attr();
                self.input.key.insert(stringify!(#sort_key).to_owned(), key_attr);
                self
            }

            fn build_expression(&mut self) -> (String, ::raiden::AttributeNames , ::raiden::AttributeValues) {
                let mut attr_names: ::raiden::AttributeNames = std::collections::HashMap::new();
                let mut attr_values: ::raiden::AttributeValues = std::collections::HashMap::new();

                let add_items = std::mem::replace(&mut self.add_items, vec![]);
                let set_items = std::mem::replace(&mut self.set_items, vec![]);
                let remove_items = std::mem::replace(&mut self.remove_items, vec![]);
                let delete_items = std::mem::replace(&mut self.delete_items, vec![]);

                let set_expression = set_items.into_iter().map(|(name, value)| {
                    let placeholder = format!(":value{}", ::raiden::generate_value_id());
                    let attr_name = format!("#{}", name.into_attr_name());
                    attr_names.insert(attr_name.clone(), name.into_attr_name());
                    attr_values.insert(placeholder.clone(), value);
                    format!("{} = {}", attr_name.clone(), placeholder)
                }).collect::<Vec<_>>().join(", ");

                let add_expression = add_items.into_iter().map(|(name, value)| {
                    let placeholder = format!(":value{}", ::raiden::generate_value_id());
                    let attr_name = format!("#{}", name.into_attr_name());
                    attr_names.insert(attr_name.clone(), name.into_attr_name());
                    attr_values.insert(placeholder.clone(), value);
                    format!("{} {}", attr_name.clone(), placeholder)
                }).collect::<Vec<_>>().join(", ");

                let remove_expression = remove_items.into_iter().map(|name| {
                    let placeholder = format!(":value{}", ::raiden::generate_value_id());
                    let attr_name = format!("#{}", name.into_attr_name());
                    attr_names.insert(attr_name.clone(), name.into_attr_name());
                    format!("{}", attr_name.clone())
                }).collect::<Vec<_>>().join(", ");

                let delete_expression = delete_items.into_iter().map(|(name, value)| {
                    let placeholder = format!(":value{}", ::raiden::generate_value_id());
                    let attr_name = format!("#{}", name.into_attr_name());
                    attr_names.insert(attr_name.clone(), name.into_attr_name());
                    attr_values.insert(placeholder.clone(), value);
                    format!("{} {}", attr_name.clone(), placeholder)
                }).collect::<Vec<_>>().join(", ");

                let mut update_expressions: Vec<String> = vec![];
                if !add_expression.is_empty() {
                    update_expressions.push(format!("ADD {}", add_expression));
                }
                if !set_expression.is_empty() {
                    update_expressions.push(format!("SET {}", set_expression));
                }
                if !remove_expression.is_empty() {
                    update_expressions.push(format!("REMOVE {}", remove_expression));
                }
                if !delete_expression.is_empty() {
                    update_expressions.push(format!("DELETE {}", delete_expression));
                }
                let update_expression = update_expressions.join(" ");
                (update_expression, attr_names, attr_values)
            }


            pub async fn run(mut self) -> Result<(), ()> {
                let (expression, names, values) = self.build_expression();
                self.input.expression_attribute_names = Some(names);
                self.input.expression_attribute_values = Some(values);
                let res = self.client.update_item(self.input).await;
                dbg!(&res);
                Ok(())
            }
        }
    }
}

// updateExpression := " ADD ids :ids SET updatedAt = :date, #version = #version + :inc"
