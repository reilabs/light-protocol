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

/// GetTransactionWithCompressionInfoV2Post200ResponseResult : A Solana transaction with additional compression information
#[derive(Clone, Default, Debug, PartialEq, Serialize, Deserialize)]
pub struct GetTransactionWithCompressionInfoV2Post200ResponseResult {
    #[serde(rename = "compression_info", skip_serializing_if = "Option::is_none")]
    pub compression_info: Option<
        Box<models::GetTransactionWithCompressionInfoV2Post200ResponseResultCompressionInfo>,
    >,
    /// An encoded confirmed transaction with status meta
    #[serde(rename = "transaction", skip_serializing_if = "Option::is_none")]
    pub transaction: Option<serde_json::Value>,
}

impl GetTransactionWithCompressionInfoV2Post200ResponseResult {
    /// A Solana transaction with additional compression information
    pub fn new() -> GetTransactionWithCompressionInfoV2Post200ResponseResult {
        GetTransactionWithCompressionInfoV2Post200ResponseResult {
            compression_info: None,
            transaction: None,
        }
    }
}
