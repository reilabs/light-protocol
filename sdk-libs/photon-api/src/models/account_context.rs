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
pub struct AccountContext {
    #[serde(rename = "inOutputQueue")]
    pub in_output_queue: bool,
    #[serde(rename = "nullifiedInTree")]
    pub nullified_in_tree: bool,
    /// A 32-byte hash represented as a base58 string.
    #[serde(rename = "nullifier", skip_serializing_if = "Option::is_none")]
    pub nullifier: Option<String>,
    #[serde(
        rename = "nullifierQueueIndex",
        skip_serializing_if = "Option::is_none"
    )]
    pub nullifier_queue_index: Option<i32>,
    /// A Solana public key represented as a base58 string.
    #[serde(rename = "queue", skip_serializing_if = "Option::is_none")]
    pub queue: Option<String>,
    #[serde(rename = "spent")]
    pub spent: bool,
    /// A 32-byte hash represented as a base58 string.
    #[serde(rename = "txHash", skip_serializing_if = "Option::is_none")]
    pub tx_hash: Option<String>,
}

impl AccountContext {
    pub fn new(in_output_queue: bool, nullified_in_tree: bool, spent: bool) -> AccountContext {
        AccountContext {
            in_output_queue,
            nullified_in_tree,
            nullifier: None,
            nullifier_queue_index: None,
            queue: None,
            spent,
            tx_hash: None,
        }
    }
}
