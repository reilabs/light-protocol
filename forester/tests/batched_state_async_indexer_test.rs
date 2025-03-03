use std::{sync::Arc, time::Duration};

use forester::{epoch_manager::WorkReport, run_pipeline, ForesterConfig};
use forester_utils::{forester_epoch::get_epoch_phases, instructions::wait_for_indexer};
use light_batched_merkle_tree::{
    initialize_state_tree::InitStateTreeAccountsInstructionData,
    merkle_tree::BatchedMerkleTreeAccount, queue::BatchedQueueAccount,
};
use light_client::{
    indexer::{photon_indexer::PhotonIndexer, AddressWithTree, Indexer},
    rpc::{solana_rpc::SolanaRpcUrl, RpcConnection, SolanaRpcConnection},
};
use light_compressed_account::{
    address::derive_address_legacy,
    compressed_account::{CompressedAccount, MerkleContext},
    instruction_data::{compressed_proof::CompressedProof, data::NewAddressParams},
};
use light_compressed_token::process_transfer::{
    transfer_sdk::create_transfer_instruction, TokenTransferOutputData,
};
use light_hasher::Poseidon;
use light_program_test::test_env::EnvAccounts;
use light_prover_client::gnark::helpers::{LightValidatorConfig, ProverConfig, ProverMode};
use light_registry::{
    protocol_config::state::{ProtocolConfig, ProtocolConfigPda},
    utils::get_protocol_config_pda_address,
};
use light_sdk::token::TokenDataWithMerkleContext;
use light_test_utils::{
    conversions::sdk_to_program_token_data, spl::create_mint_helper_with_keypair,
    system_program::create_invoke_instruction,
};
use rand::{prelude::SliceRandom, rngs::StdRng, Rng, SeedableRng};
use serial_test::serial;
use solana_program::{native_token::LAMPORTS_PER_SOL, pubkey::Pubkey};
use solana_sdk::{
    commitment_config::CommitmentConfig,
    signature::{Keypair, Signature},
    signer::Signer,
};
use tokio::{
    sync::{mpsc, oneshot, Mutex},
    time::{sleep, timeout},
};

use crate::test_utils::{forester_config, init};

mod test_utils;

// Configuration constants
const ENABLE_TRANSACTIONS: bool = true;
const OUTPUT_ACCOUNT_NUM: usize = 2;
const MINT_TO_NUM: u64 = 5;
const BATCHES_NUM: u64 = 10;
const DEFAULT_TIMEOUT_SECONDS: u64 = 60 * 5;
const PHOTON_INDEXER_URL: &str = "http://127.0.0.1:8784";
const COMPUTE_BUDGET_LIMIT: u32 = 1_000_000;
const SLEEP_DURATION: Duration = Duration::from_secs(5);

#[tokio::test(flavor = "multi_thread", worker_threads = 32)]
#[serial]
async fn test_state_indexer_async_batched() {
    let tree_params = InitStateTreeAccountsInstructionData::test_default();

    init(Some(LightValidatorConfig {
        enable_indexer: true,
        wait_time: 10,
        prover_config: Some(ProverConfig {
            run_mode: Some(ProverMode::ForesterTest),
            circuits: vec![],
        }),
        sbf_programs: vec![],
        limit_ledger_size: Some(500000),
    }))
    .await;

    let env = EnvAccounts::get_local_test_validator_accounts();
    let mut config = forester_config();
    config.payer_keypair = env.forester.insecure_clone();
    config.derivation_pubkey = env.forester.pubkey();

    let mut rpc = setup_rpc_connection(&env.forester);
    ensure_sufficient_balance(&mut rpc, &env.forester.pubkey(), LAMPORTS_PER_SOL * 100).await;
    ensure_sufficient_balance(
        &mut rpc,
        &env.governance_authority.pubkey(),
        LAMPORTS_PER_SOL * 100,
    )
    .await;

    let mut photon_indexer = {
        let rpc = SolanaRpcConnection::new(SolanaRpcUrl::Localnet, None);
        create_photon_indexer(rpc)
    };
    let protocol_config = get_protocol_config(&mut rpc).await;

    let (initial_next_index, initial_sequence_number, pre_root) =
        get_initial_merkle_tree_state(&mut rpc, &env.batched_state_merkle_tree).await;
    println!(
        "Initial state:\n\
         next_index: {}\n\
         sequence_number: {}\n\
         batch_size: {}",
        initial_next_index,
        initial_sequence_number,
        get_batch_size(&mut rpc, &env.batched_state_merkle_tree).await
    );

    let (service_handle, shutdown_sender, mut work_report_receiver) =
        setup_forester_pipeline(&config).await;

    let active_phase_slot = get_active_phase_start_slot(&mut rpc, &protocol_config).await;
    wait_for_slot(&mut rpc, active_phase_slot).await;

    let batch_payer = Keypair::from_bytes(&[
        88, 117, 248, 40, 40, 5, 251, 124, 235, 221, 10, 212, 169, 203, 91, 203, 255, 67, 210, 150,
        87, 182, 238, 155, 87, 24, 176, 252, 157, 119, 68, 81, 148, 156, 30, 0, 60, 63, 34, 247,
        192, 120, 4, 170, 32, 149, 221, 144, 74, 244, 181, 142, 37, 197, 196, 136, 159, 196, 101,
        21, 194, 56, 163, 1,
    ])
    .unwrap();
    let legacy_payer = Keypair::from_bytes(&[
        58, 94, 30, 2, 133, 249, 254, 202, 188, 51, 184, 201, 173, 158, 211, 81, 202, 46, 41, 227,
        38, 227, 101, 115, 246, 157, 174, 33, 64, 96, 207, 87, 161, 151, 87, 233, 147, 93, 116, 35,
        227, 168, 135, 146, 45, 183, 134, 2, 97, 130, 200, 207, 211, 117, 232, 198, 233, 80, 205,
        75, 41, 148, 68, 97,
    ])
    .unwrap();

    println!("batch payer pubkey: {:?}", batch_payer.pubkey());
    println!("legacy payer pubkey: {:?}", legacy_payer.pubkey());

    ensure_sufficient_balance(&mut rpc, &legacy_payer.pubkey(), LAMPORTS_PER_SOL * 100).await;
    ensure_sufficient_balance(&mut rpc, &batch_payer.pubkey(), LAMPORTS_PER_SOL * 100).await;

    let mint_keypair = Keypair::from_bytes(&[
        87, 206, 67, 171, 178, 112, 231, 204, 169, 148, 206, 45, 217, 171, 233, 199, 226, 229, 142,
        204, 52, 3, 40, 197, 103, 125, 199, 80, 17, 18, 42, 42, 72, 237, 17, 77, 168, 248, 87, 226,
        202, 233, 163, 7, 148, 155, 201, 160, 255, 17, 124, 254, 98, 74, 111, 251, 24, 230, 93,
        130, 105, 104, 119, 110,
    ])
    .unwrap();
    let mint_pubkey = create_mint_helper_with_keypair(&mut rpc, &batch_payer, &mint_keypair).await;

    let sig = mint_to(
        &mut rpc,
        &env.batched_output_queue,
        &batch_payer,
        &mint_pubkey,
    )
    .await;
    println!("mint_to: {:?}", sig);

    let mut sender_batched_accs_counter = 0;
    let mut sender_legacy_accs_counter = 0;
    let mut sender_batched_token_counter: u64 = MINT_TO_NUM * 2;
    let mut address_counter = 0;

    print_queue_states(
        &mut rpc,
        &env.batched_state_merkle_tree,
        &env.batched_output_queue,
    )
    .await;
    wait_for_indexer(&mut rpc, &photon_indexer).await.unwrap();

    let input_compressed_accounts =
        get_token_accounts(&photon_indexer, &batch_payer.pubkey(), &mint_pubkey).await;
    validate_compressed_accounts_proof(&photon_indexer, &input_compressed_accounts).await;

    let rng_seed = rand::thread_rng().gen::<u64>();
    println!("seed {}", rng_seed);
    let rng = &mut StdRng::seed_from_u64(rng_seed);

    if ENABLE_TRANSACTIONS {
        execute_test_transactions(
            &mut rpc,
            &mut photon_indexer,
            rng,
            &env,
            &batch_payer,
            &legacy_payer,
            &mint_pubkey,
            &mut sender_batched_accs_counter,
            &mut sender_legacy_accs_counter,
            &mut sender_batched_token_counter,
            &mut address_counter,
        )
        .await;
    }

    wait_for_work_report(&mut work_report_receiver, &tree_params).await;
    verify_root_changed(&mut rpc, &env.batched_state_merkle_tree, &pre_root).await;
    shutdown_sender
        .send(())
        .expect("Failed to send shutdown signal");
    service_handle.await.unwrap().unwrap();
}

// ─────────────────────────────────────────────────────────────────────────────
// HELPER FUNCTIONS
// ─────────────────────────────────────────────────────────────────────────────

fn setup_rpc_connection(forester: &Keypair) -> SolanaRpcConnection {
    let mut rpc =
        SolanaRpcConnection::new(SolanaRpcUrl::Localnet, Some(CommitmentConfig::confirmed()));
    rpc.payer = forester.insecure_clone();
    rpc
}

async fn ensure_sufficient_balance(
    rpc: &mut SolanaRpcConnection,
    pubkey: &Pubkey,
    target_balance: u64,
) {
    if rpc.get_balance(pubkey).await.unwrap() < LAMPORTS_PER_SOL {
        rpc.airdrop_lamports(pubkey, target_balance).await.unwrap();
    }
}

fn create_photon_indexer<R: RpcConnection>(rpc: R) -> PhotonIndexer<R> {
    PhotonIndexer::new(PHOTON_INDEXER_URL.to_string(), None, rpc)
}

async fn get_protocol_config(rpc: &mut SolanaRpcConnection) -> ProtocolConfig {
    let protocol_config_pda_address = get_protocol_config_pda_address().0;
    rpc.get_anchor_account::<ProtocolConfigPda>(&protocol_config_pda_address)
        .await
        .unwrap()
        .unwrap()
        .config
}

async fn get_initial_merkle_tree_state(
    rpc: &mut SolanaRpcConnection,
    merkle_tree_pubkey: &Pubkey,
) -> (u64, u64, [u8; 32]) {
    let mut merkle_tree_account = rpc.get_account(*merkle_tree_pubkey).await.unwrap().unwrap();
    let merkle_tree = BatchedMerkleTreeAccount::state_from_bytes(
        merkle_tree_account.data.as_mut_slice(),
        &merkle_tree_pubkey.into(),
    )
    .unwrap();

    let initial_next_index = merkle_tree.get_metadata().next_index;
    let initial_sequence_number = merkle_tree.get_metadata().sequence_number;
    (
        initial_next_index,
        initial_sequence_number,
        merkle_tree.get_root().unwrap(),
    )
}

async fn get_batch_size<R: RpcConnection>(rpc: &mut R, merkle_tree_pubkey: &Pubkey) -> u64 {
    let mut merkle_tree_account = rpc.get_account(*merkle_tree_pubkey).await.unwrap().unwrap();
    let merkle_tree = BatchedMerkleTreeAccount::state_from_bytes(
        merkle_tree_account.data.as_mut_slice(),
        &merkle_tree_pubkey.into(),
    )
    .unwrap();

    merkle_tree.get_metadata().queue_batches.zkp_batch_size
}

async fn setup_forester_pipeline(
    config: &ForesterConfig,
) -> (
    tokio::task::JoinHandle<anyhow::Result<()>>,
    oneshot::Sender<()>,
    mpsc::Receiver<WorkReport>,
) {
    let (shutdown_sender, shutdown_receiver) = oneshot::channel();
    let (work_report_sender, work_report_receiver) = mpsc::channel(100);

    let rpc = SolanaRpcConnection::new(SolanaRpcUrl::Localnet, None);
    let forester_photon_indexer = create_photon_indexer(rpc);
    let service_handle = tokio::spawn(run_pipeline(
        Arc::from(config.clone()),
        None,
        None,
        Arc::new(Mutex::new(forester_photon_indexer)),
        shutdown_receiver,
        work_report_sender,
    ));

    (service_handle, shutdown_sender, work_report_receiver)
}

async fn wait_for_slot(rpc: &mut SolanaRpcConnection, target_slot: u64) {
    while rpc.get_slot().await.unwrap() < target_slot {
        println!(
            "waiting for active phase slot: {}, current slot: {}",
            target_slot,
            rpc.get_slot().await.unwrap()
        );
        sleep(Duration::from_millis(400)).await;
    }
}

async fn print_queue_states(
    rpc: &mut SolanaRpcConnection,
    merkle_tree_pubkey: &Pubkey,
    output_queue_pubkey: &Pubkey,
) {
    let mut merkle_tree_account = rpc.get_account(*merkle_tree_pubkey).await.unwrap().unwrap();
    let merkle_tree = BatchedMerkleTreeAccount::state_from_bytes(
        merkle_tree_account.data.as_mut_slice(),
        &merkle_tree_pubkey.into(),
    )
    .unwrap();
    println!("merkle tree metadata: {:?}", merkle_tree.get_metadata());

    let mut output_queue_account = rpc
        .get_account(*output_queue_pubkey)
        .await
        .unwrap()
        .unwrap();
    let output_queue =
        BatchedQueueAccount::output_from_bytes(output_queue_account.data.as_mut_slice()).unwrap();
    println!("queue metadata: {:?}", output_queue.get_metadata());
}

async fn get_token_accounts<R: RpcConnection, I: Indexer<R>>(
    indexer: &I,
    owner: &Pubkey,
    mint: &Pubkey,
) -> Vec<TokenDataWithMerkleContext> {
    let accounts = indexer
        .get_compressed_token_accounts_by_owner_v2(owner, Some(*mint))
        .await
        .unwrap();
    println!("Found {} compressed token accounts", accounts.len());
    accounts
}

async fn validate_compressed_accounts_proof<R: RpcConnection, I: Indexer<R>>(
    indexer: &I,
    input_compressed_accounts: &[TokenDataWithMerkleContext],
) {
    let compressed_account_hashes = input_compressed_accounts
        .iter()
        .map(|x| x.compressed_account.hash().unwrap())
        .collect::<Vec<[u8; 32]>>();
    println!(
        "get_validity_proof_v2 for {:?}",
        compressed_account_hashes
            .iter()
            .map(|x| bs58::encode(x).into_string())
            .collect::<Vec<_>>()
    );
    let proof = indexer
        .get_validity_proof_v2(compressed_account_hashes, vec![])
        .await
        .unwrap();
    println!("proof_for_compressed_accounts: {:?}", proof);
}

#[allow(clippy::too_many_arguments)]
async fn execute_test_transactions<R: RpcConnection, I: Indexer<R>>(
    rpc: &mut R,
    indexer: &mut I,
    rng: &mut StdRng,
    env: &EnvAccounts,
    batch_payer: &Keypair,
    legacy_payer: &Keypair,
    mint_pubkey: &Pubkey,
    sender_batched_accs_counter: &mut u64,
    sender_legacy_accs_counter: &mut u64,
    sender_batched_token_counter: &mut u64,
    address_counter: &mut u64,
) {
    let batch_size = get_batch_size(rpc, &env.batched_state_merkle_tree).await;
    println!("batch size: {}", batch_size);
    for i in 0..batch_size * BATCHES_NUM {
        let batch_compress_sig = compress(
            rpc,
            &env.batched_output_queue,
            batch_payer,
            if i == 0 { 1_000_000 } else { 10_000 },
            sender_batched_accs_counter,
        )
        .await;
        println!("{} batch compress: {:?}", i, batch_compress_sig);

        let compress_sig = compress(
            rpc,
            &env.merkle_tree_pubkey,
            legacy_payer,
            if i == 0 { 1_000_000 } else { 10_000 },
            sender_legacy_accs_counter,
        )
        .await;
        println!("{} legacy compress: {:?}", i, compress_sig);

        verify_queue_states(
            rpc,
            env,
            *sender_batched_accs_counter,
            *sender_batched_token_counter,
        )
        .await;

        let batch_transfer_sig = transfer(
            rpc,
            indexer,
            &env.batched_output_queue,
            batch_payer,
            sender_batched_accs_counter,
        )
        .await;
        println!("{} batch transfer: {:?}", i, batch_transfer_sig);
        sleep(SLEEP_DURATION).await;

        let legacy_transfer_sig = transfer(
            rpc,
            indexer,
            &env.merkle_tree_pubkey,
            legacy_payer,
            sender_legacy_accs_counter,
        )
        .await;
        println!("{} legacy transfer: {:?}", i, legacy_transfer_sig);

        let batch_transfer_token_sig = compressed_token_transfer(
            rpc,
            indexer,
            &env.batched_output_queue,
            batch_payer,
            mint_pubkey,
            sender_batched_token_counter,
        )
        .await;
        println!("{} batch token transfer: {:?}", i, batch_transfer_token_sig);
    }
    sleep(SLEEP_DURATION).await;

    let sig = create_v1_address(
        rpc,
        indexer,
        rng,
        &env.address_merkle_tree_pubkey,
        &env.address_merkle_tree_queue_pubkey,
        legacy_payer,
        address_counter,
    )
    .await;
    println!(
        "total num addresses created {}, create address: {:?}",
        address_counter, sig,
    );
}

async fn verify_queue_states<R: RpcConnection>(
    rpc: &mut R,
    env: &EnvAccounts,
    sender_batched_accs_counter: u64,
    sender_batched_token_counter: u64,
) {
    let mut output_queue_account = rpc
        .get_account(env.batched_output_queue)
        .await
        .unwrap()
        .unwrap();
    let output_queue =
        BatchedQueueAccount::output_from_bytes(output_queue_account.data.as_mut_slice()).unwrap();
    println!("output queue metadata: {:?}", output_queue.get_metadata());
    let mut input_queue_account = rpc
        .get_account(env.batched_state_merkle_tree)
        .await
        .unwrap()
        .unwrap();
    let account = BatchedMerkleTreeAccount::state_from_bytes(
        input_queue_account.data.as_mut_slice(),
        &env.batched_state_merkle_tree.into(),
    )
    .unwrap();
    println!(
        "input queue next_index: {}, output queue next_index: {} sender_batched_accs_counter: {} sender_batched_token_counter: {}",
        account.queue_batches.next_index,
        output_queue.batch_metadata.next_index,
        sender_batched_accs_counter,
        sender_batched_token_counter
    );
    assert_eq!(
        output_queue.batch_metadata.next_index - account.queue_batches.next_index,
        sender_batched_accs_counter + sender_batched_token_counter
    );
}

async fn wait_for_work_report(
    work_report_receiver: &mut mpsc::Receiver<WorkReport>,
    tree_params: &InitStateTreeAccountsInstructionData,
) {
    let timeout_duration = Duration::from_secs(DEFAULT_TIMEOUT_SECONDS);
    match timeout(timeout_duration, work_report_receiver.recv()).await {
        Ok(Some(report)) => {
            println!("Received work report: {:?}", report);
            println!(
                "Work report debug:\n\
                 reported_items: {}\n\
                 batch_size: {}\n\
                 complete_batches: {}",
                report.processed_items,
                tree_params.input_queue_zkp_batch_size,
                report.processed_items / tree_params.input_queue_zkp_batch_size as usize,
            );
            assert!(report.processed_items > 0, "No items were processed");
        }
        Ok(None) => panic!("Work report channel closed unexpectedly"),
        Err(_) => panic!("Test timed out after {:?}", timeout_duration),
    }
}

async fn verify_root_changed(
    rpc: &mut SolanaRpcConnection,
    merkle_tree_pubkey: &Pubkey,
    pre_root: &[u8; 32],
) {
    let mut merkle_tree_account = rpc.get_account(*merkle_tree_pubkey).await.unwrap().unwrap();
    let merkle_tree = BatchedMerkleTreeAccount::state_from_bytes(
        merkle_tree_account.data.as_mut_slice(),
        &merkle_tree_pubkey.into(),
    )
    .unwrap();
    println!("merkle tree metadata: {:?}", merkle_tree.get_metadata());
    assert_ne!(
        *pre_root,
        merkle_tree.get_root().unwrap(),
        "Root should have changed"
    );
}

pub async fn get_active_phase_start_slot<R: RpcConnection>(
    rpc: &mut R,
    protocol_config: &ProtocolConfig,
) -> u64 {
    let current_slot = rpc.get_slot().await.unwrap();
    let current_epoch = protocol_config.get_current_epoch(current_slot);
    let phases = get_epoch_phases(protocol_config, current_epoch);
    phases.active.start
}

// ─────────────────────────────────────────────────────────────────────────────
// TRANSACTION OPERATIONS
// ─────────────────────────────────────────────────────────────────────────────

async fn mint_to<R: RpcConnection>(
    rpc: &mut R,
    merkle_tree_pubkey: &Pubkey,
    payer: &Keypair,
    mint_pubkey: &Pubkey,
) -> Signature {
    let mint_to_ix = light_compressed_token::process_mint::mint_sdk::create_mint_to_instruction(
        &payer.pubkey(),
        &payer.pubkey(),
        mint_pubkey,
        merkle_tree_pubkey,
        vec![100_000; 10],
        vec![payer.pubkey(); 10],
        None,
        false,
        0,
    );
    let instructions = vec![
        solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_limit(
            COMPUTE_BUDGET_LIMIT,
        ),
        mint_to_ix,
    ];
    rpc.create_and_send_transaction(&instructions, &payer.pubkey(), &[payer])
        .await
        .unwrap()
}

async fn compressed_token_transfer<R: RpcConnection, I: Indexer<R>>(
    rpc: &mut R,
    indexer: &I,
    merkle_tree_pubkey: &Pubkey,
    payer: &Keypair,
    mint: &Pubkey,
    counter: &mut u64,
) -> Signature {
    wait_for_indexer(rpc, indexer).await.unwrap();
    let mut input_compressed_accounts = indexer
        .get_compressed_token_accounts_by_owner_v2(&payer.pubkey(), Some(*mint))
        .await
        .unwrap();
    println!(
        "get_compressed_accounts_by_owner_v2({:?}): input_compressed_accounts: {:?}",
        payer.pubkey(),
        input_compressed_accounts
    );
    assert_eq!(
        std::cmp::min(input_compressed_accounts.len(), 1000),
        std::cmp::min(*counter as usize, 1000)
    );
    let rng = &mut rand::thread_rng();
    let num_inputs = rng.gen_range(1..2);
    input_compressed_accounts.shuffle(rng);
    input_compressed_accounts.truncate(num_inputs);
    let tokens = input_compressed_accounts
        .iter()
        .map(|x| x.token_data.amount)
        .sum::<u64>();
    let compressed_account_hashes = input_compressed_accounts
        .iter()
        .map(|x| {
            println!("compressed_account hash: {:?}", x.compressed_account.hash());
            println!("merkle_context: {:?}", x.compressed_account.merkle_context);
            x.compressed_account.hash().unwrap()
        })
        .collect::<Vec<[u8; 32]>>();
    wait_for_indexer(rpc, indexer).await.unwrap();
    let proof_for_compressed_accounts = indexer
        .get_validity_proof_v2(compressed_account_hashes, vec![])
        .await
        .unwrap();
    let root_indices = proof_for_compressed_accounts
        .root_indices
        .iter()
        .zip(input_compressed_accounts.iter_mut())
        .map(|(root_index, _)| {
            if root_index.prove_by_index {
                None
            } else {
                Some(root_index.root_index)
            }
        })
        .collect::<Vec<Option<u16>>>();
    let merkle_contexts = input_compressed_accounts
        .iter()
        .map(|x| x.compressed_account.merkle_context)
        .collect::<Vec<MerkleContext>>();
    let tokens_divided = tokens / OUTPUT_ACCOUNT_NUM as u64;
    let tokens_remained = tokens % OUTPUT_ACCOUNT_NUM as u64;
    let mut compressed_accounts = vec![
        TokenTransferOutputData {
            amount: tokens_divided,
            owner: payer.pubkey(),
            lamports: None,
            merkle_tree: *merkle_tree_pubkey,
        };
        OUTPUT_ACCOUNT_NUM
    ];
    compressed_accounts[0].amount += tokens_remained;
    println!(
        "transfer input_compressed_accounts: {:?}",
        input_compressed_accounts
    );
    println!("transfer compressed_accounts: {:?}", compressed_accounts);
    let proof = if root_indices.iter().all(|x| x.is_none()) {
        None
    } else {
        proof_for_compressed_accounts
            .compressed_proof
            .map(|proof| CompressedProof {
                a: proof.a.try_into().unwrap(),
                b: proof.b.try_into().unwrap(),
                c: proof.c.try_into().unwrap(),
            })
    };
    let input_token_data = input_compressed_accounts
        .iter()
        .map(|x| sdk_to_program_token_data(x.token_data.clone()))
        .collect::<Vec<_>>();
    let input_compressed_accounts_data = input_compressed_accounts
        .iter()
        .map(|x| x.compressed_account.compressed_account.clone())
        .collect::<Vec<_>>();
    let instruction = create_transfer_instruction(
        &payer.pubkey(),
        &payer.pubkey(),
        &merkle_contexts,
        &compressed_accounts,
        &root_indices,
        &proof,
        &input_token_data,
        &input_compressed_accounts_data,
        *mint,
        None,
        false,
        None,
        None,
        None,
        true,
        None,
        None,
        false,
        &[],
    )
    .unwrap();
    println!(
        "transfer compressed_accounts: {:?}",
        input_compressed_accounts_data
    );
    println!("transfer root_indices: {:?}", root_indices);
    let mut instructions = vec![
        solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_limit(
            COMPUTE_BUDGET_LIMIT,
        ),
    ];
    instructions.push(instruction);
    let sig = rpc
        .create_and_send_transaction(&instructions, &payer.pubkey(), &[payer])
        .await
        .unwrap();
    *counter += OUTPUT_ACCOUNT_NUM as u64;
    *counter -= input_compressed_accounts_data.len() as u64;
    sig
}

async fn transfer<R: RpcConnection, I: Indexer<R>>(
    rpc: &mut R,
    indexer: &I,
    merkle_tree_pubkey: &Pubkey,
    payer: &Keypair,
    counter: &mut u64,
) -> Signature {
    wait_for_indexer(rpc, indexer).await.unwrap();
    let mut input_compressed_accounts = indexer
        .get_compressed_accounts_by_owner_v2(&payer.pubkey())
        .await
        .unwrap_or(vec![]);
    println!(
        "get_compressed_accounts_by_owner_v2({:?}): input_compressed_accounts: {:?}",
        payer.pubkey(),
        input_compressed_accounts
    );
    assert_eq!(
        std::cmp::min(input_compressed_accounts.len(), 1000),
        std::cmp::min(*counter as usize, 1000)
    );
    let rng = &mut rand::thread_rng();
    let num_inputs = rng.gen_range(1..2);
    input_compressed_accounts.shuffle(rng);
    input_compressed_accounts.truncate(num_inputs);
    let lamports = input_compressed_accounts
        .iter()
        .map(|x| x.compressed_account.lamports)
        .sum::<u64>();
    let compressed_account_hashes = input_compressed_accounts
        .iter()
        .map(|x| {
            x.compressed_account
                .hash::<Poseidon>(
                    &x.merkle_context.merkle_tree_pubkey,
                    &x.merkle_context.leaf_index,
                )
                .unwrap()
        })
        .collect::<Vec<[u8; 32]>>();
    wait_for_indexer(rpc, indexer).await.unwrap();
    let proof_for_compressed_accounts = indexer
        .get_validity_proof_v2(compressed_account_hashes, vec![])
        .await
        .unwrap();
    let root_indices = proof_for_compressed_accounts
        .root_indices
        .iter()
        .zip(input_compressed_accounts.iter_mut())
        .map(|(root_index, acc)| {
            if root_index.prove_by_index {
                acc.merkle_context.prove_by_index = true;
                None
            } else {
                acc.merkle_context.prove_by_index = false;
                Some(root_index.root_index)
            }
        })
        .collect::<Vec<Option<u16>>>();
    let merkle_contexts = input_compressed_accounts
        .iter()
        .map(|x| x.merkle_context)
        .collect::<Vec<MerkleContext>>();
    let lamp = lamports / OUTPUT_ACCOUNT_NUM as u64;
    let lamport_remained = lamports % OUTPUT_ACCOUNT_NUM as u64;
    let mut compressed_accounts = vec![
        CompressedAccount {
            lamports: lamp,
            owner: payer.pubkey(),
            address: None,
            data: None,
        };
        OUTPUT_ACCOUNT_NUM
    ];
    compressed_accounts[0].lamports += lamport_remained;
    println!(
        "transfer input_compressed_accounts: {:?}",
        input_compressed_accounts
    );
    println!("transfer compressed_accounts: {:?}", compressed_accounts);
    let proof = if root_indices.iter().all(|x| x.is_none()) {
        None
    } else {
        proof_for_compressed_accounts
            .compressed_proof
            .map(|proof| CompressedProof {
                a: proof.a.try_into().unwrap(),
                b: proof.b.try_into().unwrap(),
                c: proof.c.try_into().unwrap(),
            })
    };
    let input_compressed_accounts_data = input_compressed_accounts
        .iter()
        .map(|x| x.compressed_account.clone())
        .collect::<Vec<CompressedAccount>>();
    let instruction = create_invoke_instruction(
        &payer.pubkey(),
        &payer.pubkey(),
        &input_compressed_accounts_data,
        &compressed_accounts,
        &merkle_contexts,
        &[*merkle_tree_pubkey; OUTPUT_ACCOUNT_NUM],
        &root_indices,
        &[],
        proof,
        None,
        false,
        None,
        true,
    );
    println!(
        "transfer compressed_accounts: {:?}",
        input_compressed_accounts_data
    );
    println!("transfer root_indices: {:?}", root_indices);
    let mut instructions = vec![
        solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_limit(
            COMPUTE_BUDGET_LIMIT,
        ),
    ];
    instructions.push(instruction);
    let sig = rpc
        .create_and_send_transaction(&instructions, &payer.pubkey(), &[payer])
        .await
        .unwrap();
    *counter += OUTPUT_ACCOUNT_NUM as u64;
    *counter -= input_compressed_accounts_data.len() as u64;
    sig
}

async fn compress<R: RpcConnection>(
    rpc: &mut R,
    merkle_tree_pubkey: &Pubkey,
    payer: &Keypair,
    lamports: u64,
    counter: &mut u64,
) -> Signature {
    let compress_account = CompressedAccount {
        lamports,
        owner: payer.pubkey(),
        address: None,
        data: None,
    };
    let instruction = create_invoke_instruction(
        &payer.pubkey(),
        &payer.pubkey(),
        &[],
        &[compress_account],
        &[],
        &[*merkle_tree_pubkey],
        &[],
        &[],
        None,
        Some(lamports),
        true,
        None,
        true,
    );
    println!("compress instruction: {:?}", instruction);
    let mut instructions = vec![
        solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_limit(
            COMPUTE_BUDGET_LIMIT,
        ),
    ];
    instructions.push(instruction);
    let sig = rpc
        .create_and_send_transaction(&instructions, &payer.pubkey(), &[payer])
        .await
        .unwrap();
    *counter += 1;
    sig
}

async fn create_v1_address<R: RpcConnection, I: Indexer<R>>(
    rpc: &mut R,
    indexer: &mut I,
    rng: &mut StdRng,
    merkle_tree_pubkey: &Pubkey,
    queue: &Pubkey,
    payer: &Keypair,
    counter: &mut u64,
) -> Signature {
    let num_addresses = rng.gen_range(1..=2);
    let mut address_proof_inputs = Vec::new();
    let mut seeds = Vec::new();
    for _ in 0..num_addresses {
        let seed = rng.gen::<[u8; 32]>();
        seeds.push(seed);
        let address = derive_address_legacy(merkle_tree_pubkey, &seed).unwrap();
        address_proof_inputs.push(AddressWithTree {
            address,
            tree: *merkle_tree_pubkey,
        });
    }
    wait_for_indexer(rpc, indexer).await.unwrap();
    let proof_for_addresses = indexer
        .get_validity_proof_v2(vec![], address_proof_inputs)
        .await
        .unwrap();
    let mut new_address_params = Vec::new();
    for (seed, root_index) in seeds.iter().zip(proof_for_addresses.root_indices.iter()) {
        assert!(
            !root_index.prove_by_index,
            "Addresses have no proof by index."
        );
        new_address_params.push(NewAddressParams {
            seed: *seed,
            address_queue_pubkey: *queue,
            address_merkle_tree_pubkey: *merkle_tree_pubkey,
            address_merkle_tree_root_index: root_index.root_index,
        });
    }
    let proof = proof_for_addresses
        .compressed_proof
        .map(|proof| CompressedProof {
            a: proof.a.try_into().unwrap(),
            b: proof.b.try_into().unwrap(),
            c: proof.c.try_into().unwrap(),
        });
    let instruction = create_invoke_instruction(
        &payer.pubkey(),
        &payer.pubkey(),
        &[],
        &[],
        &[],
        &[],
        &proof_for_addresses
            .root_indices
            .iter()
            .map(|x| Some(x.root_index))
            .collect::<Vec<_>>(),
        &new_address_params,
        proof,
        None,
        false,
        None,
        false,
    );
    println!("create address instruction: {:?}", instruction);
    let mut instructions = vec![
        solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_limit(
            COMPUTE_BUDGET_LIMIT,
        ),
    ];
    instructions.push(instruction);
    let sig = rpc
        .create_and_send_transaction(&instructions, &payer.pubkey(), &[payer])
        .await
        .unwrap();
    *counter += 1;
    sig
}
