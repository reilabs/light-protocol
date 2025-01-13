/*
 * photon-indexer
 *
 * Solana indexer for general compression
 *
 * The version of the OpenAPI document: 0.45.0
 *
 * Generated by: https://openapi-generator.tech
 */

use crate::models;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum AccountState {
    #[serde(rename = "initialized")]
    Initialized,
    #[serde(rename = "frozen")]
    Frozen,
}

impl std::fmt::Display for AccountState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Initialized => write!(f, "initialized"),
            Self::Frozen => write!(f, "frozen"),
        }
    }
}

impl Default for AccountState {
    fn default() -> AccountState {
        Self::Initialized
    }
}
