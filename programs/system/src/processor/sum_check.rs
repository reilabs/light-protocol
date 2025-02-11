use anchor_lang::{solana_program::program_error::ProgramError, Result};
use light_compressed_account::instruction_data::zero_copy::{
    ZOutputCompressedAccountWithPackedContext, ZPackedCompressedAccountWithMerkleContext,
};

use crate::errors::SystemProgramError;

#[inline(always)]
pub fn sum_check(
    input_compressed_accounts_with_merkle_context: &[ZPackedCompressedAccountWithMerkleContext<
        '_,
    >],
    output_compressed_accounts: &[ZOutputCompressedAccountWithPackedContext<'_>],
    relay_fee: &Option<u64>,
    compress_or_decompress_lamports: &Option<u64>,
    is_compress: &bool,
) -> Result<usize> {
    let mut sum: u64 = 0;
    let mut num_prove_by_index_accounts = 0;
    for compressed_account_with_context in input_compressed_accounts_with_merkle_context.iter() {
        if compressed_account_with_context
            .merkle_context
            .prove_by_index()
        {
            num_prove_by_index_accounts += 1;
        }

        sum = sum
            .checked_add(u64::from(
                compressed_account_with_context.compressed_account.lamports,
            ))
            .ok_or(ProgramError::ArithmeticOverflow)
            .map_err(|_| SystemProgramError::ComputeInputSumFailed)?;
    }

    if let Some(lamports) = compress_or_decompress_lamports {
        if *is_compress {
            sum = sum
                .checked_add(*lamports)
                .ok_or(ProgramError::ArithmeticOverflow)
                .map_err(|_| SystemProgramError::ComputeOutputSumFailed)?;
        } else {
            sum = sum
                .checked_sub(*lamports)
                .ok_or(ProgramError::ArithmeticOverflow)
                .map_err(|_| SystemProgramError::ComputeOutputSumFailed)?;
        }
    }

    for compressed_account in output_compressed_accounts.iter() {
        sum = sum
            .checked_sub(u64::from(compressed_account.compressed_account.lamports))
            .ok_or(ProgramError::ArithmeticOverflow)
            .map_err(|_| SystemProgramError::ComputeOutputSumFailed)?;
    }

    if let Some(relay_fee) = relay_fee {
        sum = sum
            .checked_sub(*relay_fee)
            .ok_or(ProgramError::ArithmeticOverflow)
            .map_err(|_| SystemProgramError::ComputeRpcSumFailed)?;
    }

    if sum == 0 {
        Ok(num_prove_by_index_accounts)
    } else {
        Err(SystemProgramError::SumCheckFailed.into())
    }
}

#[cfg(test)]
mod test {
    use anchor_lang::AnchorSerialize;
    use light_compressed_account::{
        compressed_account::{
            CompressedAccount, PackedCompressedAccountWithMerkleContext, PackedMerkleContext,
        },
        instruction_data::data::OutputCompressedAccountWithPackedContext,
        pubkey::Pubkey,
    };
    use light_zero_copy::borsh::Deserialize;

    use super::*;

    #[test]
    fn test_sum_check() {
        // SUCCEED: no relay fee, compression
        sum_check_test(&[100, 50], &[150], None, None, false).unwrap();
        sum_check_test(&[75, 25, 25], &[25, 25, 25, 25, 12, 13], None, None, false).unwrap();

        // FAIL: no relay fee, compression
        sum_check_test(&[100, 50], &[150 + 1], None, None, false).unwrap_err();
        sum_check_test(&[100, 50], &[150 - 1], None, None, false).unwrap_err();
        sum_check_test(&[100, 50], &[], None, None, false).unwrap_err();
        sum_check_test(&[], &[100, 50], None, None, false).unwrap_err();
        sum_check_test(&[100, 50], &[0], None, None, false).unwrap_err();
        sum_check_test(&[0], &[100, 50], None, None, false).unwrap_err();

        // SUCCEED: empty
        sum_check_test(&[], &[], None, None, true).unwrap();
        sum_check_test(&[], &[], None, None, false).unwrap();
        sum_check_test(&[0], &[0], None, None, true).unwrap();
        sum_check_test(&[0], &[0], None, None, false).unwrap();
        // FAIL: empty
        sum_check_test(&[], &[], Some(1), None, false).unwrap_err();
        sum_check_test(&[], &[], None, Some(1), false).unwrap_err();
        sum_check_test(&[], &[], None, Some(1), true).unwrap_err();

        // SUCCEED: with compress
        sum_check_test(&[100], &[123], None, Some(23), true).unwrap();
        sum_check_test(&[], &[150], None, Some(150), true).unwrap();
        // FAIL: compress
        sum_check_test(&[], &[150], None, Some(150 - 1), true).unwrap_err();
        sum_check_test(&[], &[150], None, Some(150 + 1), true).unwrap_err();

        // SUCCEED: with decompress
        sum_check_test(&[100, 50], &[100], None, Some(50), false).unwrap();
        sum_check_test(&[100, 50], &[], None, Some(150), false).unwrap();
        // FAIL: decompress
        sum_check_test(&[100, 50], &[], None, Some(150 - 1), false).unwrap_err();
        sum_check_test(&[100, 50], &[], None, Some(150 + 1), false).unwrap_err();

        // SUCCEED: with relay fee
        sum_check_test(&[100, 50], &[125], Some(25), None, false).unwrap();
        sum_check_test(&[100, 50], &[150], Some(25), Some(25), true).unwrap();
        sum_check_test(&[100, 50], &[100], Some(25), Some(25), false).unwrap();

        // FAIL: relay fee
        sum_check_test(&[100, 50], &[2125], Some(25 - 1), None, false).unwrap_err();
        sum_check_test(&[100, 50], &[2125], Some(25 + 1), None, false).unwrap_err();
        for i in 0..10 {
            sum_check_test_with_num(&vec![150; i], &vec![150; i], None, None, false, i).unwrap();
        }
    }
    fn sum_check_test(
        input_amounts: &[u64],
        output_amounts: &[u64],
        relay_fee: Option<u64>,
        compress_or_decompress_lamports: Option<u64>,
        is_compress: bool,
    ) -> Result<()> {
        sum_check_test_with_num(
            input_amounts,
            output_amounts,
            relay_fee,
            compress_or_decompress_lamports,
            is_compress,
            0,
        )
    }
    fn sum_check_test_with_num(
        input_amounts: &[u64],
        output_amounts: &[u64],
        relay_fee: Option<u64>,
        compress_or_decompress_lamports: Option<u64>,
        is_compress: bool,
        num_by_index: usize,
    ) -> Result<()> {
        let mut bytes = Vec::new();

        let mut inputs = Vec::new();
        for (index, i) in input_amounts.iter().enumerate() {
            let prove_by_index = index < num_by_index;
            let merkle_context = PackedMerkleContext {
                merkle_tree_pubkey_index: 0,
                nullifier_queue_pubkey_index: 0,
                leaf_index: 0,
                prove_by_index,
            };
            inputs.push(PackedCompressedAccountWithMerkleContext {
                compressed_account: CompressedAccount {
                    owner: Pubkey::new_unique().into(),
                    lamports: *i,
                    address: None,
                    data: None,
                },
                merkle_context,
                root_index: 1,
                read_only: false,
            });
            // let mut _bytes = Vec::new();
            inputs
                .last()
                .unwrap()
                .serialize(&mut bytes)
                .map_err(|_| ProgramError::InvalidArgument)?;
            // bytes.push(_bytes);
        }
        let mut output_bytes = Vec::new();
        let mut outputs = Vec::new();
        for amount in output_amounts.iter() {
            outputs.push(OutputCompressedAccountWithPackedContext {
                compressed_account: CompressedAccount {
                    owner: Pubkey::new_unique().into(),
                    lamports: *amount,
                    address: None,
                    data: None,
                },
                merkle_tree_index: 0,
            });
            outputs
                .last()
                .unwrap()
                .serialize(&mut output_bytes)
                .map_err(|_| ProgramError::InvalidArgument)?;
        }

        let mut slice = bytes.as_slice();
        let mut inputs = Vec::new();
        for _ in 0..input_amounts.len() {
            let (input, _bytes) =
                ZPackedCompressedAccountWithMerkleContext::zero_copy_at(slice).unwrap();
            slice = _bytes;
            inputs.push(input);
        }
        let mut slice = output_bytes.as_slice();
        let mut outputs = Vec::new();
        for _ in 0..output_amounts.len() {
            let (output, _bytes) =
                ZOutputCompressedAccountWithPackedContext::zero_copy_at(slice).unwrap();
            slice = _bytes;
            outputs.push(output);
        }

        let calc_num_prove_by_index_accounts = sum_check(
            inputs.as_slice(),
            outputs.as_slice(),
            &relay_fee,
            &compress_or_decompress_lamports,
            &is_compress,
        )?;

        assert_eq!(num_by_index, calc_num_prove_by_index_accounts);
        Ok(())
    }
}
