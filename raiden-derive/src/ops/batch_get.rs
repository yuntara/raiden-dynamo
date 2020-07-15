use quote::*;

pub(crate) fn expand_batch_get(
    partition_key: &proc_macro2::Ident,
    sort_key: &Option<proc_macro2::Ident>,
    struct_name: &proc_macro2::Ident,
    fields: &syn::FieldsNamed,
    rename_all_type: crate::rename::RenameAllType,
) -> proc_macro2::TokenStream {
    let trait_name = format_ident!("{}BatchGetItem", struct_name);
    let client_name = format_ident!("{}Client", struct_name);
    let builder_name = format_ident!("{}BatchGetItemBuilder", struct_name);
    let from_item = super::expand_attr_to_item(&format_ident!("res_item"), fields, rename_all_type);

    let builder_keys_type = if sort_key.is_none() {
        quote! { std::vec::Vec<::raiden::AttributeValue> }
    } else {
        quote! { std::vec::Vec<(::raiden::AttributeValue, ::raiden::AttributeValue)> }
    };

    let builder_init = quote! {
        #builder_name {
            client: &self.client,
            table_name: self.table_name(),
            keys: key_attrs,
        }
    };

    let client_trait = if sort_key.is_none() {
        quote! {
            pub trait #trait_name {
                fn batch_get<K>(&self, keys: std::vec::Vec<K>) -> #builder_name
                    where K: ::raiden::IntoAttribute + std::marker::Send;
            }

            impl #trait_name for #client_name {
                fn batch_get<K>(&self, keys: std::vec::Vec<K>) -> #builder_name
                    where K: ::raiden::IntoAttribute + std::marker::Send
                {
                    let mut key_attrs = vec![];
                    for key in keys.into_iter() {
                        key_attrs.push(key.into_attr());
                    }

                    #builder_init
                }
            }
        }
    } else {
        quote! {
            pub trait #trait_name {
                fn batch_get<PK, SK>(&self, keys: std::vec::Vec<(PK, SK)>) -> #builder_name
                    where PK: ::raiden::IntoAttribute + std::marker::Send,
                          SK: ::raiden::IntoAttribute + std::marker::Send;
            }

            impl #trait_name for #client_name {
                fn batch_get<PK, SK>(&self, keys: std::vec::Vec<(PK, SK)>) -> #builder_name
                    where PK: ::raiden::IntoAttribute + std::marker::Send,
                          SK: ::raiden::IntoAttribute + std::marker::Send
                {
                    let mut key_attrs = vec![];
                    for (pk, sk) in keys.into_iter() {
                        key_attrs.push((pk.into_attr(), sk.into_attr()));
                    }

                    #builder_init
                }
            }
        }
    };

    let convert_to_external_proc = if let Some(sort_key) = sort_key {
        quote! {
            for (pk_attr, sk_attr) in keys.into_iter() {
                let mut key_val: std::collections::HashMap<String, ::raiden::AttributeValue> = Default::default();
                key_val.insert(stringify!(#partition_key).to_owned(), pk_attr);
                key_val.insert(stringify!(#sort_key).to_owned(), sk_attr);
                item.keys.push(key_val);
            }
        }
    } else {
        quote! {
            for key_attr in keys.into_iter() {
                let mut key_val: std::collections::HashMap<String, ::raiden::AttributeValue> = Default::default();
                key_val.insert(stringify!(#partition_key).to_owned(), key_attr);
                item.keys.push(key_val);
            }
        }
    };

    quote! {
        #client_trait

        pub struct #builder_name<'a> {
            pub client: &'a ::raiden::DynamoDbClient,
            pub table_name: String,
            pub keys: #builder_keys_type,
        }

        impl<'a> #builder_name<'a> {
            async fn run(mut self) -> Result<::raiden::batch_get::BatchGetOutput<#struct_name>, ::raiden::RaidenError> {
                let mut items: std::vec::Vec<#struct_name> = vec![];
                let mut unprocessed_keys = ::raiden::KeysAndAttributes::default();

                loop {
                    let mut input = ::raiden::BatchGetItemInput::default();

                    let mut item = ::raiden::KeysAndAttributes::default();
                    item.keys = Default::default();

                    let keys = self.keys.drain(0..std::cmp::min(100, self.keys.len()));
                    #convert_to_external_proc

                    input.request_items = Default::default();
                    input
                        .request_items
                        .insert(self.table_name.to_string(), item);

                    let res = self.client.batch_get_item(input).await?;
                    if let Some(res_responses) = &res.responses {
                        if let Some(res_items) = res_responses.get(&self.table_name) {
                            for res_item in res_items.iter() {
                                items.push(#struct_name {
                                    #(#from_item)*
                                })
                            }
                        } else {
                            return Err(::raiden::RaidenError::ResourceNotFound(format!("'{}' table not found or not active", &self.table_name)));
                        }
                    } else {
                        return Err(::raiden::RaidenError::ResourceNotFound("resource not found".to_owned()));
                    }

                    if let Some(mut keys_by_table) = res.unprocessed_keys {
                        if let Some(mut keys_attrs) = keys_by_table.get_mut(&self.table_name) {
                            unprocessed_keys.keys.append(&mut keys_attrs.keys);
                        }
                    }

                    // TODO: required somehing to prevent infinite-loop
                    if self.keys.is_empty() {
                        return Ok(::raiden::batch_get::BatchGetOutput {
                            consumed_capacity: res.consumed_capacity,
                            items,
                            unprocessed_keys: Some(unprocessed_keys),
                        })
                    }
                }
            }
        }
    }
}

/*
https://docs.aws.amazon.com/ja_jp/sdk-for-javascript/v2/developer-guide/dynamodb-example-table-read-write-batch.html

pub struct BatchGetItemInput {
    /// <p><p>A map of one or more table names and, for each table, a map that describes one or more items to retrieve from that table. Each table name can be used only once per <code>BatchGetItem</code> request.</p> <p>Each element in the map of items to retrieve consists of the following:</p> <ul> <li> <p> <code>ConsistentRead</code> - If <code>true</code>, a strongly consistent read is used; if <code>false</code> (the default), an eventually consistent read is used.</p> </li> <li> <p> <code>ExpressionAttributeNames</code> - One or more substitution tokens for attribute names in the <code>ProjectionExpression</code> parameter. The following are some use cases for using <code>ExpressionAttributeNames</code>:</p> <ul> <li> <p>To access an attribute whose name conflicts with a DynamoDB reserved word.</p> </li> <li> <p>To create a placeholder for repeating occurrences of an attribute name in an expression.</p> </li> <li> <p>To prevent special characters in an attribute name from being misinterpreted in an expression.</p> </li> </ul> <p>Use the <b>#</b> character in an expression to dereference an attribute name. For example, consider the following attribute name:</p> <ul> <li> <p> <code>Percentile</code> </p> </li> </ul> <p>The name of this attribute conflicts with a reserved word, so it cannot be used directly in an expression. (For the complete list of reserved words, see <a href="https://docs.aws.amazon.com/amazondynamodb/latest/developerguide/ReservedWords.html">Reserved Words</a> in the <i>Amazon DynamoDB Developer Guide</i>). To work around this, you could specify the following for <code>ExpressionAttributeNames</code>:</p> <ul> <li> <p> <code>{&quot;#P&quot;:&quot;Percentile&quot;}</code> </p> </li> </ul> <p>You could then use this substitution in an expression, as in this example:</p> <ul> <li> <p> <code>#P = :val</code> </p> </li> </ul> <note> <p>Tokens that begin with the <b>:</b> character are <i>expression attribute values</i>, which are placeholders for the actual value at runtime.</p> </note> <p>For more information about expression attribute names, see <a href="https://docs.aws.amazon.com/amazondynamodb/latest/developerguide/Expressions.AccessingItemAttributes.html">Accessing Item Attributes</a> in the <i>Amazon DynamoDB Developer Guide</i>.</p> </li> <li> <p> <code>Keys</code> - An array of primary key attribute values that define specific items in the table. For each primary key, you must provide <i>all</i> of the key attributes. For example, with a simple primary key, you only need to provide the partition key value. For a composite key, you must provide <i>both</i> the partition key value and the sort key value.</p> </li> <li> <p> <code>ProjectionExpression</code> - A string that identifies one or more attributes to retrieve from the table. These attributes can include scalars, sets, or elements of a JSON document. The attributes in the expression must be separated by commas.</p> <p>If no attribute names are specified, then all attributes are returned. If any of the requested attributes are not found, they do not appear in the result.</p> <p>For more information, see <a href="https://docs.aws.amazon.com/amazondynamodb/latest/developerguide/Expressions.AccessingItemAttributes.html">Accessing Item Attributes</a> in the <i>Amazon DynamoDB Developer Guide</i>.</p> </li> <li> <p> <code>AttributesToGet</code> - This is a legacy parameter. Use <code>ProjectionExpression</code> instead. For more information, see <a href="https://docs.aws.amazon.com/amazondynamodb/latest/developerguide/LegacyConditionalParameters.AttributesToGet.html">AttributesToGet</a> in the <i>Amazon DynamoDB Developer Guide</i>. </p> </li> </ul></p>
    #[serde(rename = "RequestItems")]
    pub request_items: ::std::collections::HashMap<String, KeysAndAttributes>,
    #[serde(rename = "ReturnConsumedCapacity")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_consumed_capacity: Option<String>,
}

pub struct KeysAndAttributes {
    pub consistent_read: Option<bool>,
    pub expression_attribute_names: Option<::std::collections::HashMap<String, String>>,
    pub keys: Vec<::std::collections::HashMap<String, AttributeValue>>,
    pub projection_expression: Option<String>,
}

pub struct BatchGetItemOutput {
    /// <p><p>The read capacity units consumed by the entire <code>BatchGetItem</code> operation.</p> <p>Each element consists of:</p> <ul> <li> <p> <code>TableName</code> - The table that consumed the provisioned throughput.</p> </li> <li> <p> <code>CapacityUnits</code> - The total number of capacity units consumed.</p> </li> </ul></p>
    #[serde(rename = "ConsumedCapacity")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consumed_capacity: Option<Vec<ConsumedCapacity>>,
    /// <p>A map of table name to a list of items. Each object in <code>Responses</code> consists of a table name, along with a map of attribute data consisting of the data type and attribute value.</p>
    #[serde(rename = "Responses")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub responses: Option<
        ::std::collections::HashMap<
            String,
            Vec<::std::collections::HashMap<String, AttributeValue>>,
        >,
    >,
    /// <p>A map of tables and their respective keys that were not processed with the current response. The <code>UnprocessedKeys</code> value is in the same form as <code>RequestItems</code>, so the value can be provided directly to a subsequent <code>BatchGetItem</code> operation. For more information, see <code>RequestItems</code> in the Request Parameters section.</p> <p>Each element consists of:</p> <ul> <li> <p> <code>Keys</code> - An array of primary key attribute values that define specific items in the table.</p> </li> <li> <p> <code>ProjectionExpression</code> - One or more attributes to be retrieved from the table or index. By default, all attributes are returned. If a requested attribute is not found, it does not appear in the result.</p> </li> <li> <p> <code>ConsistentRead</code> - The consistency of a read operation. If set to <code>true</code>, then a strongly consistent read is used; otherwise, an eventually consistent read is used.</p> </li> </ul> <p>If there are no unprocessed keys remaining, the response contains an empty <code>UnprocessedKeys</code> map.</p>
    #[serde(rename = "UnprocessedKeys")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unprocessed_keys: Option<::std::collections::HashMap<String, KeysAndAttributes>>,
}

*/
