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
pub struct Context {
    #[serde(rename = "slot")]
    pub slot: u64,
}

impl Context {
    pub fn new(slot: u64) -> Context {
        Context { slot }
    }
}
