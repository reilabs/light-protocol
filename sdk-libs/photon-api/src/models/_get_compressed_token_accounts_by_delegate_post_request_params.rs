/*
 * photon-indexer
 *
 * Solana indexer for general compression
 *
 * The version of the OpenAPI document: 0.50.0
 *
 * Generated by: https://openapi-generator.tech
 */

use crate::models;

#[derive(Clone, Default, Debug, PartialEq, Serialize, Deserialize)]
pub struct GetCompressedTokenAccountsByDelegatePostRequestParams {
    /// A base 58 encoded string.
    #[serde(
        rename = "cursor",
        default,
        with = "::serde_with::rust::double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub cursor: Option<Option<String>>,
    /// A Solana public key represented as a base58 string.
    #[serde(rename = "delegate")]
    pub delegate: String,
    #[serde(
        rename = "limit",
        default,
        with = "::serde_with::rust::double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub limit: Option<Option<i32>>,
    /// A Solana public key represented as a base58 string.
    #[serde(
        rename = "mint",
        default,
        with = "::serde_with::rust::double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub mint: Option<Option<String>>,
}

impl GetCompressedTokenAccountsByDelegatePostRequestParams {
    pub fn new(delegate: String) -> GetCompressedTokenAccountsByDelegatePostRequestParams {
        GetCompressedTokenAccountsByDelegatePostRequestParams {
            cursor: None,
            delegate,
            limit: None,
            mint: None,
        }
    }
}
