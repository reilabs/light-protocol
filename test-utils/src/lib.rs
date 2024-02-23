use anchor_lang::{
    solana_program::{pubkey::Pubkey, system_instruction},
    AnchorDeserialize,
};
use solana_program_test::{BanksClientError, ProgramTestContext};
use solana_sdk::{
    account::Account, instruction::Instruction, signature::Keypair, signer::Signer,
    transaction::Transaction,
};

pub mod test_env;

pub struct AccountZeroCopy<'a, T> {
    pub account: Account,
    pub deserialized: &'a T,
    phantom_data: std::marker::PhantomData<T>,
}

impl<'a, T> AccountZeroCopy<'a, T> {
    pub async fn new(context: &mut ProgramTestContext, address: Pubkey) -> AccountZeroCopy<'a, T> {
        let account = context
            .banks_client
            .get_account(address)
            .await
            .unwrap()
            .unwrap();
        unsafe {
            let ptr = account.data[8..].as_ptr() as *const T;
            Self {
                account,
                deserialized: &*ptr,
                phantom_data: std::marker::PhantomData,
            }
        }
    }
}

pub async fn get_account<T: AnchorDeserialize>(
    context: &mut ProgramTestContext,
    pubkey: Pubkey,
) -> T {
    let account = context
        .banks_client
        .get_account(pubkey)
        .await
        .unwrap()
        .unwrap();
    T::deserialize(&mut &account.data[8..]).unwrap()
}

pub async fn airdrop_lamports(
    banks_client: &mut ProgramTestContext,
    destination_pubkey: &Pubkey,
    lamports: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create a transfer instruction
    let transfer_instruction =
        system_instruction::transfer(&banks_client.payer.pubkey(), destination_pubkey, lamports);

    // Create and sign a transaction
    let transaction = Transaction::new_signed_with_payer(
        &[transfer_instruction],
        Some(&banks_client.payer.pubkey()),
        &vec![&banks_client.payer],
        banks_client.last_blockhash,
    );

    // Send the transaction
    banks_client
        .banks_client
        .process_transaction(transaction)
        .await?;

    Ok(())
}

pub async fn create_and_send_transaction(
    context: &mut ProgramTestContext,
    instruction: &[Instruction],
    payer: &Keypair,
) -> Result<(), BanksClientError> {
    let transaction = Transaction::new_signed_with_payer(
        instruction,
        Some(&payer.pubkey()),
        &vec![&payer],
        context.last_blockhash,
    );
    context.banks_client.process_transaction(transaction).await
}