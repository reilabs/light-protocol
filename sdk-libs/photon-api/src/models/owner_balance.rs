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
pub struct OwnerBalance {
    #[serde(rename = "balance")]
    pub balance: i32,
    /// A Solana public key represented as a base58 string.
    #[serde(rename = "owner")]
    pub owner: String,
}

impl OwnerBalance {
    pub fn new(balance: i32, owner: String) -> OwnerBalance {
        OwnerBalance { balance, owner }
    }
}
