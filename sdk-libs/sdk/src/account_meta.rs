use light_compressed_account::compressed_account::{
    CompressedAccountWithMerkleContext, PackedMerkleContext,
};

use crate::{
    error::LightSdkError,
    merkle_context::{pack_merkle_context, CpiAccounts},
    BorshDeserialize, BorshSerialize,
};

/// InputAccountMeta (context, address, root_index, output_merkle_tree_index)
/// InputAccountMetaNoLamportsNoAddress (context, root_index, output_merkle_tree_index)
/// InputAccountMetaWithLamportsNoAddress (context, root_index, output_merkle_tree_index)
/// InputAccountMetaWithLamports (context, lamports, address, root_index, output_merkle_tree_index)
pub trait InputAccountMetaTrait {
    fn get_merkle_context(&self) -> &PackedMerkleContext;
    fn get_lamports(&self) -> Option<u64>;
    fn get_root_index(&self) -> Option<u16>;
    fn get_address(&self) -> Option<[u8; 32]>;
    fn get_output_merkle_tree_index(&self) -> u8;
}

#[derive(Default, Debug, Clone, Copy, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct InputAccountMetaNoLamportsNoAddress {
    pub merkle_context: PackedMerkleContext,
    pub output_merkle_tree_index: u8,
    pub root_index: Option<u16>,
}

impl InputAccountMetaTrait for InputAccountMetaNoLamportsNoAddress {
    fn get_merkle_context(&self) -> &PackedMerkleContext {
        &self.merkle_context
    }

    fn get_lamports(&self) -> Option<u64> {
        None
    }

    fn get_root_index(&self) -> Option<u16> {
        self.root_index
    }

    fn get_address(&self) -> Option<[u8; 32]> {
        None
    }

    fn get_output_merkle_tree_index(&self) -> u8 {
        self.output_merkle_tree_index
    }
}

impl InputAccountMetaNoLamportsNoAddress {
    pub fn from_compressed_account(
        compressed_account: &CompressedAccountWithMerkleContext,
        cpi_accounts: &mut CpiAccounts,
        root_index: Option<u16>,
        output_merkle_tree: &solana_program::pubkey::Pubkey,
    ) -> Self {
        let mut merkle_context =
            pack_merkle_context(&compressed_account.merkle_context, cpi_accounts);
        let output_merkle_tree_index = cpi_accounts.insert_or_get(*output_merkle_tree);
        if root_index.is_none() {
            merkle_context.prove_by_index = true;
        }
        InputAccountMetaNoLamportsNoAddress {
            merkle_context,
            root_index,
            output_merkle_tree_index,
        }
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct InputAccountMetaNoAddress {
    pub merkle_context: PackedMerkleContext,
    pub output_merkle_tree_index: u8,
    pub lamports: u64,
    pub root_index: Option<u16>,
}

impl InputAccountMetaTrait for InputAccountMetaNoAddress {
    fn get_merkle_context(&self) -> &PackedMerkleContext {
        &self.merkle_context
    }

    fn get_lamports(&self) -> Option<u64> {
        Some(self.lamports)
    }

    fn get_root_index(&self) -> Option<u16> {
        self.root_index
    }

    fn get_address(&self) -> Option<[u8; 32]> {
        None
    }

    fn get_output_merkle_tree_index(&self) -> u8 {
        self.output_merkle_tree_index
    }
}

impl InputAccountMetaNoAddress {
    pub fn from_compressed_account(
        compressed_account: &CompressedAccountWithMerkleContext,
        cpi_accounts: &mut CpiAccounts,
        root_index: Option<u16>,
        output_merkle_tree: &solana_program::pubkey::Pubkey,
    ) -> Self {
        let mut merkle_context =
            pack_merkle_context(&compressed_account.merkle_context, cpi_accounts);

        let output_merkle_tree_index = cpi_accounts.insert_or_get(*output_merkle_tree);
        if root_index.is_none() {
            merkle_context.prove_by_index = true;
        }
        InputAccountMetaNoAddress {
            merkle_context,
            root_index,
            output_merkle_tree_index,
            lamports: compressed_account.compressed_account.lamports,
        }
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct InputAccountMeta {
    /// Merkle tree context.
    pub merkle_context: PackedMerkleContext,
    /// Address.
    pub address: [u8; 32],
    /// Root index.
    pub root_index: Option<u16>,
    pub output_merkle_tree_index: u8,
}

impl InputAccountMetaTrait for InputAccountMeta {
    fn get_merkle_context(&self) -> &PackedMerkleContext {
        &self.merkle_context
    }

    fn get_lamports(&self) -> Option<u64> {
        None
    }

    fn get_root_index(&self) -> Option<u16> {
        self.root_index
    }

    fn get_address(&self) -> Option<[u8; 32]> {
        Some(self.address)
    }

    fn get_output_merkle_tree_index(&self) -> u8 {
        self.output_merkle_tree_index
    }
}

impl InputAccountMeta {
    pub fn from_compressed_account(
        compressed_account: &CompressedAccountWithMerkleContext,
        cpi_accounts: &mut CpiAccounts,
        root_index: Option<u16>,
        output_merkle_tree: &solana_program::pubkey::Pubkey,
    ) -> Result<Self, LightSdkError> {
        let mut merkle_context =
            pack_merkle_context(&compressed_account.merkle_context, cpi_accounts);

        let address = compressed_account
            .compressed_account
            .address
            .ok_or(LightSdkError::MissingField("address".to_string()))?;

        let output_merkle_tree_index = cpi_accounts.insert_or_get(*output_merkle_tree);

        if root_index.is_none() {
            merkle_context.prove_by_index = true;
        }
        Ok(InputAccountMeta {
            merkle_context,
            address,
            root_index,
            output_merkle_tree_index,
        })
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct InputAccountMetaWithLamports {
    /// Merkle tree context.
    pub merkle_context: PackedMerkleContext,
    /// Lamports.
    pub lamports: u64,
    /// Address.
    pub address: [u8; 32],
    /// Root index.
    pub output_merkle_tree_index: u8,
    pub root_index: Option<u16>,
}

impl InputAccountMetaTrait for InputAccountMetaWithLamports {
    fn get_merkle_context(&self) -> &PackedMerkleContext {
        &self.merkle_context
    }

    fn get_lamports(&self) -> Option<u64> {
        Some(self.lamports)
    }

    fn get_root_index(&self) -> Option<u16> {
        self.root_index
    }

    fn get_address(&self) -> Option<[u8; 32]> {
        Some(self.address)
    }

    fn get_output_merkle_tree_index(&self) -> u8 {
        self.output_merkle_tree_index
    }
}

impl InputAccountMetaWithLamports {
    pub fn from_compressed_account(
        compressed_account: &CompressedAccountWithMerkleContext,
        cpi_accounts: &mut CpiAccounts,
        root_index: Option<u16>,
        output_merkle_tree: &solana_program::pubkey::Pubkey,
    ) -> Result<Self, LightSdkError> {
        let mut merkle_context =
            pack_merkle_context(&compressed_account.merkle_context, cpi_accounts);

        // Use the address if available, otherwise default
        let address = compressed_account
            .compressed_account
            .address
            .ok_or(LightSdkError::MissingField("address".to_string()))?;
        let output_merkle_tree_index = cpi_accounts.insert_or_get(*output_merkle_tree);
        if root_index.is_none() {
            merkle_context.prove_by_index = true;
        }
        Ok(InputAccountMetaWithLamports {
            merkle_context,
            lamports: compressed_account.compressed_account.lamports,
            address,
            root_index,
            output_merkle_tree_index,
        })
    }
}
