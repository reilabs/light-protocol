#![cfg(feature = "test-sbf")]
use std::collections::HashMap;

use account_compression::{
    self,
    errors::AccountCompressionErrorCode,
    sdk::{create_initialize_merkle_tree_instruction, create_insert_leaves_instruction},
    state::{queue_from_bytes_zero_copy_mut, QueueAccount},
    utils::constants::{
        STATE_MERKLE_TREE_CANOPY_DEPTH, STATE_MERKLE_TREE_CHANGELOG, STATE_MERKLE_TREE_HEIGHT,
        STATE_MERKLE_TREE_ROOTS, STATE_NULLIFIER_QUEUE_INDICES, STATE_NULLIFIER_QUEUE_VALUES,
    },
    NullifierQueueConfig, QueueType, StateMerkleTreeAccount, StateMerkleTreeConfig, ID,
};
use anchor_lang::{error::ErrorCode, system_program, InstructionData, ToAccountMetas};
use light_concurrent_merkle_tree::{event::MerkleTreeEvent, ConcurrentMerkleTree26};
use light_hash_set::HashSetError;
use light_hasher::{zero_bytes::poseidon::ZERO_BYTES, Hasher, Poseidon};
use light_merkle_tree_reference::MerkleTree;
use light_test_utils::rpc::test_rpc::ProgramTestRpcConnection;
use light_test_utils::{
    airdrop_lamports,
    assert_merkle_tree::assert_merkle_tree_initialized,
    create_account_instruction, get_hash_set,
    state_tree_rollover::{
        assert_rolled_over_pair, perform_state_merkle_tree_roll_over,
        set_state_merkle_tree_next_index,
    },
    AccountZeroCopy,
};
use light_test_utils::{
    assert_queue::assert_nullifier_queue_initialized,
    rpc::errors::{assert_rpc_error, RpcError},
};
use light_test_utils::{
    rpc::rpc_connection::RpcConnection, test_env::create_address_merkle_tree_and_queue_account,
};
use light_utils::bigint::bigint_to_be_bytes_array;
use memoffset::offset_of;
use num_bigint::ToBigUint;
use solana_program_test::ProgramTest;
use solana_sdk::{
    account::AccountSharedData,
    instruction::{AccountMeta, Instruction},
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use solana_sdk::{account::WritableAccount, pubkey::Pubkey};

/// Tests:
/// 1. Functional: Initialize nullifier queue
/// 2. Functional: Insert into nullifier queue
/// 3. Failing: Insert the same elements into nullifier queue again (3 and 1 element(s))
/// 4. Failing: Insert into nullifier queue with invalid authority
/// 5. Functional: Insert one element into nullifier queue
#[tokio::test]
async fn test_init_and_insert_into_nullifier_queue() {
    let mut program_test = ProgramTest::default();
    program_test.add_program("account_compression", ID, None);
    program_test.add_program(
        "spl_noop",
        Pubkey::new_from_array(account_compression::utils::constants::NOOP_PUBKEY),
        None,
    );
    let merkle_tree_keypair = Keypair::new();
    let merkle_tree_pubkey = merkle_tree_keypair.pubkey();
    let nullifier_queue_keypair = Keypair::new();
    let nullifier_queue_pubkey = nullifier_queue_keypair.pubkey();
    program_test.set_compute_max_units(1_400_000u64);
    let context = program_test.start_with_context().await;
    let mut rpc = ProgramTestRpcConnection { context };
    let payer_pubkey = rpc.get_payer().pubkey();
    let network_fee = 123;
    let rollover_threshold = Some(95);
    let close_threshold = Some(100);
    functional_1_initialize_state_merkle_tree_and_nullifier_queue(
        &mut rpc,
        &payer_pubkey,
        &merkle_tree_keypair,
        &nullifier_queue_keypair,
        network_fee,
        rollover_threshold,
        close_threshold,
    )
    .await;
    let merkle_tree_keypair_2 = Keypair::new();
    let nullifier_queue_keypair_2 = Keypair::new();
    functional_1_initialize_state_merkle_tree_and_nullifier_queue(
        &mut rpc,
        &payer_pubkey,
        &merkle_tree_keypair_2,
        &nullifier_queue_keypair_2,
        network_fee,
        rollover_threshold,
        close_threshold,
    )
    .await;
    functional_2_test_insert_into_nullifier_queues(
        &mut rpc,
        &nullifier_queue_pubkey,
        &merkle_tree_pubkey,
    )
    .await;

    fail_3_insert_same_elements_into_nullifier_queue(
        &mut rpc,
        &nullifier_queue_pubkey,
        &merkle_tree_pubkey,
        vec![[3u8; 32], [1u8; 32], [1u8; 32]],
    )
    .await;
    fail_3_insert_same_elements_into_nullifier_queue(
        &mut rpc,
        &nullifier_queue_pubkey,
        &merkle_tree_pubkey,
        vec![[1u8; 32]],
    )
    .await;
    fail_4_insert_with_invalid_signer(
        &mut rpc,
        &nullifier_queue_pubkey,
        &merkle_tree_pubkey,
        vec![[3u8; 32]],
    )
    .await;

    functional_5_test_insert_into_nullifier_queue(
        &mut rpc,
        &nullifier_queue_pubkey,
        &merkle_tree_pubkey,
    )
    .await;
    let queue_tree_pair = (nullifier_queue_pubkey, merkle_tree_pubkey);
    let queue_tree_pair_2 = (
        nullifier_queue_keypair_2.pubkey(),
        merkle_tree_keypair_2.pubkey(),
    );
    let nullifier_1 = [10u8; 32];
    let nullifier_2 = [20u8; 32];
    // CHECK: nullifiers inserted into correct queue with 2 queues
    functional_6_test_insert_into_two_nullifier_queues(
        &mut rpc,
        &vec![nullifier_1, nullifier_2],
        &[queue_tree_pair, queue_tree_pair_2],
    )
    .await;

    let nullifier_1 = [11u8; 32];
    let nullifier_2 = [21u8; 32];
    let nullifier_3 = [31u8; 32];
    let nullifier_4 = [41u8; 32];
    // CHECK: nullifiers inserted into correct queue with 2 queues and not ordered
    functional_7_test_insert_into_two_nullifier_queues_not_ordered(
        &mut rpc,
        &vec![nullifier_1, nullifier_2, nullifier_3, nullifier_4],
        &[
            queue_tree_pair,
            queue_tree_pair_2,
            queue_tree_pair,
            queue_tree_pair_2,
        ],
    )
    .await;
}

/// Tests:
/// (Since nullifier queue and address queue use the same code, we only need to test one)
/// Show that we cannot insert into a full queue.
/// 1. try to insert into queue to generate the full error
/// 2. nullify one
/// 3. try to insert again it should still generate the full error
/// 4. advance Merkle tree seq until one before it would work check that it still fails
/// 5. advance Merkle tree seq by one and check that inserting works now
/// 6.try inserting again it should fail with full error
#[tokio::test]
async fn test_full_nullifier_queue() {
    let mut program_test = ProgramTest::default();
    program_test.add_program("account_compression", ID, None);
    program_test.add_program(
        "spl_noop",
        Pubkey::new_from_array(account_compression::utils::constants::NOOP_PUBKEY),
        None,
    );
    let merkle_tree_keypair = Keypair::new();
    let merkle_tree_pubkey = merkle_tree_keypair.pubkey();
    let nullifier_queue_keypair = Keypair::new();
    let nullifier_queue_pubkey = nullifier_queue_keypair.pubkey();
    program_test.set_compute_max_units(1_400_000u64);
    let context = program_test.start_with_context().await;
    let mut rpc = ProgramTestRpcConnection { context };
    let payer_pubkey = rpc.get_payer().pubkey();
    let tip = 123;
    let rollover_threshold = Some(95);
    let close_threshold = Some(100);
    functional_1_initialize_state_merkle_tree_and_nullifier_queue(
        &mut rpc,
        &payer_pubkey,
        &merkle_tree_keypair,
        &nullifier_queue_keypair,
        tip,
        rollover_threshold.clone(),
        close_threshold,
    )
    .await;
    let leaf: [u8; 32] = bigint_to_be_bytes_array(&1.to_biguint().unwrap()).unwrap();
    // append a leaf so that we have a leaf to nullify
    let mut reference_merkle_tree_1 = ConcurrentMerkleTree26::<Poseidon>::new(
        STATE_MERKLE_TREE_HEIGHT as usize,
        STATE_MERKLE_TREE_CHANGELOG as usize,
        STATE_MERKLE_TREE_ROOTS as usize,
        STATE_MERKLE_TREE_CANOPY_DEPTH as usize,
    )
    .unwrap();
    reference_merkle_tree_1.init().unwrap();
    functional_3_append_leaves_to_merkle_tree(
        &mut rpc,
        &mut [&mut reference_merkle_tree_1],
        &vec![merkle_tree_pubkey],
        &vec![(0u8, leaf)],
    )
    .await;
    let lamports_queue_accounts = rpc
        .get_account(nullifier_queue_pubkey)
        .await
        .unwrap()
        .unwrap()
        .lamports
        + rpc
            .get_account(merkle_tree_pubkey)
            .await
            .unwrap()
            .unwrap()
            .lamports
            * 2;
    // fills queue with increasing values starting from 0
    // -> in this process inserts leaf with value 1 into queue
    // all elements are marked with sequence number 2400
    set_nullifier_queue_to_full(
        &mut rpc,
        &nullifier_queue_pubkey,
        0,
        lamports_queue_accounts,
    )
    .await;

    let initial_value = 6005;
    let element: [u8; 32] = bigint_to_be_bytes_array(&initial_value.to_biguint().unwrap()).unwrap();
    // CHECK 1
    fail_insert_into_full_queue(
        &mut rpc,
        &nullifier_queue_pubkey,
        &merkle_tree_pubkey,
        vec![element],
    )
    .await;
    let mut reference_merkle_tree = MerkleTree::<Poseidon>::new(26, 10);
    reference_merkle_tree.append(&leaf).unwrap();
    let onchain_merkle_tree =
        AccountZeroCopy::<StateMerkleTreeAccount>::new(&mut rpc, merkle_tree_pubkey).await;
    let deserialized = onchain_merkle_tree.deserialized();
    let merkle_tree = deserialized.copy_merkle_tree().unwrap();
    assert_eq!(merkle_tree.root(), reference_merkle_tree.root());
    let leaf_index = reference_merkle_tree.get_leaf_index(&leaf).unwrap() as u64;
    // CHECK 2
    nullify(
        &mut rpc,
        &merkle_tree_pubkey,
        &nullifier_queue_pubkey,
        &mut reference_merkle_tree,
        &leaf,
        merkle_tree.changelog_index() as u64,
        1,
        leaf_index,
    )
    .await
    .unwrap();
    // CHECK 3
    fail_insert_into_full_queue(
        &mut rpc,
        &nullifier_queue_pubkey,
        &merkle_tree_pubkey,
        vec![element],
    )
    .await;
    // advance to sequence number minus one
    set_state_merkle_tree_sequence(&mut rpc, &merkle_tree_pubkey, 2402, lamports_queue_accounts)
        .await;
    // CHECK 4
    fail_insert_into_full_queue(
        &mut rpc,
        &nullifier_queue_pubkey,
        &merkle_tree_pubkey,
        vec![element],
    )
    .await;
    // TODO: add e2e test in compressed pda program for this
    set_state_merkle_tree_sequence(&mut rpc, &merkle_tree_pubkey, 2403, lamports_queue_accounts)
        .await;
    let payer = rpc.get_payer().insecure_clone();
    let account = rpc
        .get_account(nullifier_queue_pubkey)
        .await
        .unwrap()
        .unwrap();

    let mut data = account.data.clone();
    let nullifier_queue = &mut unsafe { queue_from_bytes_zero_copy_mut(&mut data).unwrap() };
    let replacement_start_value = 606;
    let replacement_value = find_overlapping_probe_index(
        1,
        replacement_start_value,
        nullifier_queue.hash_set.capacity_values,
    );
    // CHECK: 5
    let element: [u8; 32] =
        bigint_to_be_bytes_array(&replacement_value.to_biguint().unwrap()).unwrap();
    insert_into_single_nullifier_queue(
        &vec![element],
        &payer,
        &payer,
        &nullifier_queue_pubkey,
        &merkle_tree_pubkey,
        &mut rpc,
    )
    .await
    .unwrap();
    // CHECK: 6
    let element: [u8; 32] = bigint_to_be_bytes_array(&12000.to_biguint().unwrap()).unwrap();
    fail_insert_into_full_queue(
        &mut rpc,
        &nullifier_queue_pubkey,
        &merkle_tree_pubkey,
        vec![element],
    )
    .await;
}

/// Insert nullifiers failing tests
/// Test:
/// 1. no nullifiers
/// 2. mismatch remaining accounts and addresses
/// 3. invalid queue accounts:
/// 3.1 pass non queue account as queue account
/// 3.2 pass address queue account
/// 3.3 pass non associated queue account
/// 4. invalid Merkle tree accounts:
/// 4.1 pass non Merkle tree account as Merkle tree account
/// 4.2 pass non associated Merkle tree account
#[tokio::test]
async fn failing_queue_tests() {
    let mut program_test = ProgramTest::default();
    program_test.add_program("account_compression", ID, None);
    program_test.add_program(
        "spl_noop",
        Pubkey::new_from_array(account_compression::utils::constants::NOOP_PUBKEY),
        None,
    );
    let merkle_tree_keypair = Keypair::new();
    let merkle_tree_pubkey = merkle_tree_keypair.pubkey();
    let nullifier_queue_keypair = Keypair::new();
    let nullifier_queue_pubkey = nullifier_queue_keypair.pubkey();
    program_test.set_compute_max_units(1_400_000u64);
    let context = program_test.start_with_context().await;
    let mut rpc = ProgramTestRpcConnection { context };
    let payer = rpc.get_payer().insecure_clone();
    let payer_pubkey = rpc.get_payer().pubkey();
    let network_fee = 123;
    let rollover_threshold = Some(95);
    let close_threshold = Some(100);
    functional_1_initialize_state_merkle_tree_and_nullifier_queue(
        &mut rpc,
        &payer_pubkey,
        &merkle_tree_keypair,
        &nullifier_queue_keypair,
        network_fee,
        rollover_threshold,
        close_threshold,
    )
    .await;
    let merkle_tree_keypair_2 = Keypair::new();
    let nullifier_queue_keypair_2 = Keypair::new();
    functional_1_initialize_state_merkle_tree_and_nullifier_queue(
        &mut rpc,
        &payer_pubkey,
        &merkle_tree_keypair_2,
        &nullifier_queue_keypair_2,
        network_fee,
        rollover_threshold,
        close_threshold,
    )
    .await;

    let address_merkle_tree_keypair = Keypair::new();
    let address_queue_keypair = Keypair::new();
    create_address_merkle_tree_and_queue_account(
        &payer,
        &mut rpc,
        &address_merkle_tree_keypair,
        &address_queue_keypair,
        None,
        1,
    )
    .await;

    let queue_tree_pair = (nullifier_queue_pubkey, merkle_tree_pubkey);
    // CHECK 1: no nullifiers as input
    let result =
        insert_into_nullifier_queues(&vec![], &payer, &payer, &[queue_tree_pair], &mut rpc).await;
    assert_rpc_error(
        result,
        0,
        AccountCompressionErrorCode::InputElementsEmpty.into(),
    )
    .unwrap();
    let nullifier_1 = [1u8; 32];
    // CHECK 2: Number of leaves/addresses leaves mismatch
    let result = insert_into_nullifier_queues(
        &vec![nullifier_1],
        &payer,
        &payer,
        &[queue_tree_pair, queue_tree_pair],
        &mut rpc,
    )
    .await;
    assert_rpc_error(
        result,
        0,
        AccountCompressionErrorCode::NumberOfLeavesMismatch.into(),
    )
    .unwrap();

    // CHECK 3.1: pass non queue account as queue account
    let result = insert_into_nullifier_queues(
        &vec![nullifier_1],
        &payer,
        &payer,
        &[(merkle_tree_pubkey, merkle_tree_pubkey)],
        &mut rpc,
    )
    .await;
    assert_rpc_error(result, 0, ErrorCode::AccountDiscriminatorMismatch.into()).unwrap();

    // CHECK 3.2: pass address queue account instead of nullifier queue account
    let result = insert_into_nullifier_queues(
        &vec![nullifier_1],
        &payer,
        &payer,
        &[(address_queue_keypair.pubkey(), merkle_tree_pubkey)],
        &mut rpc,
    )
    .await;
    assert_rpc_error(
        result,
        0,
        AccountCompressionErrorCode::InvalidQueueType.into(),
    )
    .unwrap();
    let nullifier_2 = [2u8; 32];

    // CHECK 3.3: pass non associated queue account
    let result = insert_into_nullifier_queues(
        &vec![nullifier_2],
        &payer,
        &payer,
        &[(nullifier_queue_keypair_2.pubkey(), merkle_tree_pubkey)],
        &mut rpc,
    )
    .await;
    assert_rpc_error(
        result,
        0,
        AccountCompressionErrorCode::MerkleTreeAndQueueNotAssociated.into(),
    )
    .unwrap();
    // CHECK 4.1: pass non Merkle tree account
    // Triggering a discriminator mismatch error is not possibly
    // by passing an invalid Merkle tree account.
    // A non Merkle tree account cannot be associated with a queue account.
    // Hence the instruction fails with MerkleTreeAndQueueNotAssociated.
    // The Merkle tree account will not be deserialized.
    let result = insert_into_nullifier_queues(
        &vec![nullifier_1],
        &payer,
        &payer,
        &[(
            nullifier_queue_keypair.pubkey(),
            nullifier_queue_keypair.pubkey(),
        )],
        &mut rpc,
    )
    .await;
    assert_rpc_error(
        result,
        0,
        AccountCompressionErrorCode::MerkleTreeAndQueueNotAssociated.into(),
    )
    .unwrap();
    // CHECK 4.2: pass non associated Merkle tree account
    let result = insert_into_nullifier_queues(
        &vec![nullifier_1],
        &payer,
        &payer,
        &[(
            nullifier_queue_keypair.pubkey(),
            merkle_tree_keypair_2.pubkey(),
        )],
        &mut rpc,
    )
    .await;
    assert_rpc_error(
        result,
        0,
        AccountCompressionErrorCode::MerkleTreeAndQueueNotAssociated.into(),
    )
    .unwrap();
}

/// Tests:
/// 1. Should fail: not ready for rollover
/// 2. Should fail: merkle tree and queue not associated (invalid tree)
/// 3. Should fail: merkle tree and queue not associated (invalid queue)
/// 4. Should succeed: rollover state merkle tree
/// 5. Should fail: merkle tree already rolled over
#[tokio::test]
async fn test_init_and_rollover_state_merkle_tree() {
    let mut program_test = ProgramTest::default();
    program_test.add_program("account_compression", ID, None);
    program_test.add_program(
        "spl_noop",
        Pubkey::new_from_array(account_compression::utils::constants::NOOP_PUBKEY),
        None,
    );
    let merkle_tree_keypair = Keypair::new();
    let merkle_tree_pubkey = merkle_tree_keypair.pubkey();
    let nullifier_queue_keypair = Keypair::new();
    let nullifier_queue_pubkey = nullifier_queue_keypair.pubkey();
    program_test.set_compute_max_units(1_400_000u64);
    let context = program_test.start_with_context().await;
    let mut context = ProgramTestRpcConnection { context };
    let payer_pubkey = context.get_payer().pubkey();
    let tip = 123;
    let rollover_threshold = Some(95);
    let close_threshold = Some(100);
    functional_1_initialize_state_merkle_tree_and_nullifier_queue(
        &mut context,
        &payer_pubkey,
        &merkle_tree_keypair,
        &nullifier_queue_keypair,
        tip,
        rollover_threshold,
        close_threshold,
    )
    .await;

    let merkle_tree_keypair_2 = Keypair::new();
    let merkle_tree_pubkey_2 = merkle_tree_keypair_2.pubkey();
    let nullifier_queue_keypair_2 = Keypair::new();
    functional_1_initialize_state_merkle_tree_and_nullifier_queue(
        &mut context,
        &payer_pubkey,
        &merkle_tree_keypair_2,
        &nullifier_queue_keypair_2,
        tip,
        rollover_threshold,
        close_threshold,
    )
    .await;

    let required_next_index = 2u64.pow(26) * rollover_threshold.unwrap() / 100;
    let failing_next_index = required_next_index - 1;
    let lamports_queue_accounts = context
        .get_account(nullifier_queue_pubkey)
        .await
        .unwrap()
        .unwrap()
        .lamports
        + context
            .get_account(merkle_tree_pubkey)
            .await
            .unwrap()
            .unwrap()
            .lamports
            * 2;
    set_state_merkle_tree_next_index(
        &mut context,
        &merkle_tree_pubkey,
        failing_next_index,
        lamports_queue_accounts,
    )
    .await;

    let new_nullifier_queue_keypair = Keypair::new();
    let new_state_merkle_tree_keypair = Keypair::new();

    let result = perform_state_merkle_tree_roll_over(
        &mut context,
        &new_nullifier_queue_keypair,
        &new_state_merkle_tree_keypair,
        &merkle_tree_pubkey,
        &nullifier_queue_pubkey,
    )
    .await;

    assert_rpc_error(
        result,
        2,
        AccountCompressionErrorCode::NotReadyForRollover.into(),
    )
    .unwrap();

    set_state_merkle_tree_next_index(
        &mut context,
        &merkle_tree_pubkey,
        required_next_index,
        lamports_queue_accounts,
    )
    .await;
    let result = perform_state_merkle_tree_roll_over(
        &mut context,
        &new_nullifier_queue_keypair,
        &new_state_merkle_tree_keypair,
        &merkle_tree_pubkey,
        &nullifier_queue_keypair_2.pubkey(),
    )
    .await;

    assert_rpc_error(
        result,
        2,
        AccountCompressionErrorCode::MerkleTreeAndQueueNotAssociated.into(),
    )
    .unwrap();

    let result = perform_state_merkle_tree_roll_over(
        &mut context,
        &new_nullifier_queue_keypair,
        &new_state_merkle_tree_keypair,
        &merkle_tree_pubkey_2,
        &nullifier_queue_keypair.pubkey(),
    )
    .await;

    assert_rpc_error(
        result,
        2,
        AccountCompressionErrorCode::MerkleTreeAndQueueNotAssociated.into(),
    )
    .unwrap();

    let signer_prior_balance = context
        .get_account(payer_pubkey)
        .await
        .unwrap()
        .unwrap()
        .lamports;

    perform_state_merkle_tree_roll_over(
        &mut context,
        &new_nullifier_queue_keypair,
        &new_state_merkle_tree_keypair,
        &merkle_tree_pubkey,
        &nullifier_queue_pubkey,
    )
    .await
    .unwrap();

    assert_rolled_over_pair(
        &mut context,
        &signer_prior_balance,
        &merkle_tree_pubkey,
        &nullifier_queue_pubkey,
        &new_state_merkle_tree_keypair.pubkey(),
        &new_nullifier_queue_keypair.pubkey(),
    )
    .await;

    let failing_new_nullifier_queue_keypair = Keypair::new();
    let failing_new_state_merkle_tree_keypair = Keypair::new();

    let result = perform_state_merkle_tree_roll_over(
        &mut context,
        &failing_new_nullifier_queue_keypair,
        &failing_new_state_merkle_tree_keypair,
        &merkle_tree_pubkey,
        &nullifier_queue_pubkey,
    )
    .await;

    assert_rpc_error(
        result,
        2,
        AccountCompressionErrorCode::MerkleTreeAlreadyRolledOver.into(),
    )
    .unwrap();
}

/// Tests:
/// 1. Functional: Initialize merkle tree
/// 2. Failing: mismatching leaf and merkle tree accounts number
/// 3. Failing: pass invalid Merkle tree account
/// 4. Functional: Append leaves to merkle tree
/// 5. Functional: Append leaves to multiple merkle trees not-ordered
/// 6. Failing: Append leaves with invalid authority
#[tokio::test]
async fn test_append_funtional_and_failing() {
    let mut program_test = ProgramTest::default();
    program_test.add_program("account_compression", ID, None);
    program_test.add_program(
        "spl_noop",
        Pubkey::new_from_array(account_compression::utils::constants::NOOP_PUBKEY),
        None,
    );

    program_test.set_compute_max_units(1_400_000u64);
    let context = program_test.start_with_context().await;
    let mut context = ProgramTestRpcConnection { context };
    let payer_pubkey = context.get_payer().pubkey();
    let merkle_tree_keypair = Keypair::new();
    let queue_keypair = Keypair::new();
    // CHECK 1
    let merkle_tree_pubkey = functional_1_initialize_state_merkle_tree_and_nullifier_queue(
        &mut context,
        &payer_pubkey,
        &merkle_tree_keypair,
        &queue_keypair,
        0,
        None,
        None,
    )
    .await;
    let merkle_tree_keypair_2 = Keypair::new();
    let queue_keypair_2 = Keypair::new();
    let merkle_tree_pubkey_2 = functional_1_initialize_state_merkle_tree_and_nullifier_queue(
        &mut context,
        &payer_pubkey,
        &merkle_tree_keypair_2,
        &queue_keypair_2,
        1,
        None,
        None,
    )
    .await;

    // CHECK: 2 fail append with invalid inputs (mismatching leaf and merkle tree accounts)
    fail_2_append_leaves_with_invalid_inputs(
        &mut context,
        &[merkle_tree_pubkey],
        vec![(0, [1u8; 32]), (1, [2u8; 32])],
        AccountCompressionErrorCode::NotAllLeavesProcessed.into(),
    )
    .await
    .unwrap();
    // CHECK: 3 fail append with invalid inputs (pass invalid Merkle tree account)
    fail_2_append_leaves_with_invalid_inputs(
        &mut context,
        &[queue_keypair.pubkey()],
        vec![(0, [1u8; 32])],
        ErrorCode::AccountDiscriminatorMismatch.into(),
    )
    .await
    .unwrap();

    // CHECK: 4 append leaves to merkle tree
    let leaves = (0u8..=140)
        .map(|i| {
            (
                0,
                [
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, i,
                ],
            )
        })
        .collect::<Vec<(u8, [u8; 32])>>();
    let mut reference_merkle_tree_1 = ConcurrentMerkleTree26::<Poseidon>::new(
        STATE_MERKLE_TREE_HEIGHT as usize,
        STATE_MERKLE_TREE_CHANGELOG as usize,
        STATE_MERKLE_TREE_ROOTS as usize,
        STATE_MERKLE_TREE_CANOPY_DEPTH as usize,
    )
    .unwrap();
    reference_merkle_tree_1.init().unwrap();
    functional_3_append_leaves_to_merkle_tree(
        &mut context,
        &mut [&mut reference_merkle_tree_1],
        &vec![merkle_tree_pubkey],
        &leaves,
    )
    .await;

    let leaves = vec![
        (0, [1u8; 32]),
        (1, [2u8; 32]),
        (2, [3u8; 32]),
        (3, [4u8; 32]),
    ];
    let mut reference_merkle_tree_2 = ConcurrentMerkleTree26::<Poseidon>::new(
        STATE_MERKLE_TREE_HEIGHT as usize,
        STATE_MERKLE_TREE_CHANGELOG as usize,
        STATE_MERKLE_TREE_ROOTS as usize,
        STATE_MERKLE_TREE_CANOPY_DEPTH as usize,
    )
    .unwrap();
    reference_merkle_tree_2.init().unwrap();
    // CHECK: 5 append leaves to multiple merkle trees not-ordered
    functional_3_append_leaves_to_merkle_tree(
        &mut context,
        &mut [&mut reference_merkle_tree_1, &mut reference_merkle_tree_2],
        &vec![
            merkle_tree_pubkey,
            merkle_tree_pubkey_2,
            merkle_tree_pubkey,
            merkle_tree_pubkey_2,
        ],
        &leaves,
    )
    .await;

    // CHECK 6: fail append with invalid authority
    fail_4_append_leaves_with_invalid_authority(&mut context, &merkle_tree_pubkey).await;
}

/// Tests:
/// 1. Functional: nullify leaf
/// 2. Failing: nullify leaf with invalid leaf index
/// 3. Failing: nullify leaf with invalid leaf queue index
/// 4. Failing: nullify leaf with invalid change log index
/// 5. Functional: nullify other leaf
/// 6. Failing: nullify leaf with nullifier queue that is not associated with the merkle tree
#[tokio::test]
async fn test_nullify_leaves() {
    let mut program_test = ProgramTest::default();
    program_test.add_program("account_compression", ID, None);
    program_test.add_program(
        "spl_noop",
        Pubkey::new_from_array(account_compression::utils::constants::NOOP_PUBKEY),
        None,
    );
    let merkle_tree_keypair = Keypair::new();
    let merkle_tree_pubkey = merkle_tree_keypair.pubkey();
    let nullifier_queue_keypair = Keypair::new();
    let nullifier_queue_pubkey = nullifier_queue_keypair.pubkey();
    program_test.set_compute_max_units(1_400_000u64);
    let context = program_test.start_with_context().await;
    let mut context = ProgramTestRpcConnection { context };
    let payer = context.get_payer().insecure_clone();
    let payer_pubkey = context.get_payer().pubkey();
    let tip = 123;
    let rollover_threshold = Some(95);
    let close_threshold = Some(100);
    functional_1_initialize_state_merkle_tree_and_nullifier_queue(
        &mut context,
        &payer_pubkey,
        &merkle_tree_keypair,
        &nullifier_queue_keypair,
        tip,
        rollover_threshold,
        close_threshold,
    )
    .await;

    let other_merkle_tree_keypair = Keypair::new();
    let invalid_nullifier_queue_keypair = Keypair::new();
    let invalid_nullifier_queue_pubkey = nullifier_queue_keypair.pubkey();
    functional_1_initialize_state_merkle_tree_and_nullifier_queue(
        &mut context,
        &payer_pubkey,
        &other_merkle_tree_keypair,
        &invalid_nullifier_queue_keypair,
        tip,
        rollover_threshold,
        close_threshold,
    )
    .await;

    let elements = vec![(0, [1u8; 32]), (0, [2u8; 32])];
    let mut reference_merkle_tree = ConcurrentMerkleTree26::<Poseidon>::new(
        STATE_MERKLE_TREE_HEIGHT as usize,
        STATE_MERKLE_TREE_CHANGELOG as usize,
        STATE_MERKLE_TREE_ROOTS as usize,
        STATE_MERKLE_TREE_CANOPY_DEPTH as usize,
    )
    .unwrap();
    reference_merkle_tree.init().unwrap();
    functional_3_append_leaves_to_merkle_tree(
        &mut context,
        &mut [&mut reference_merkle_tree],
        &vec![merkle_tree_pubkey],
        &elements,
    )
    .await;

    insert_into_single_nullifier_queue(
        &elements.iter().map(|element| element.1).collect(),
        &payer,
        &payer,
        &nullifier_queue_pubkey,
        &merkle_tree_pubkey,
        &mut context,
    )
    .await
    .unwrap();

    let mut reference_merkle_tree = MerkleTree::<Poseidon>::new(
        STATE_MERKLE_TREE_HEIGHT as usize,
        STATE_MERKLE_TREE_CANOPY_DEPTH as usize,
    );
    reference_merkle_tree.append(&elements[0].1).unwrap();
    reference_merkle_tree.append(&elements[1].1).unwrap();

    let element_index = reference_merkle_tree
        .get_leaf_index(&elements[0].1)
        .unwrap() as u64;
    nullify(
        &mut context,
        &merkle_tree_pubkey,
        &nullifier_queue_pubkey,
        &mut reference_merkle_tree,
        &elements[0].1,
        2,
        0,
        element_index,
    )
    .await
    .unwrap();

    // nullify with invalid leaf index
    let invalid_element_index = 0;
    let valid_changelog_index = 3;
    let valid_leaf_queue_index = 1;
    nullify(
        &mut context,
        &merkle_tree_pubkey,
        &nullifier_queue_pubkey,
        &mut reference_merkle_tree,
        &elements[1].1,
        valid_changelog_index,
        valid_leaf_queue_index,
        invalid_element_index,
    )
    .await
    .unwrap_err();
    let valid_element_index = 1;
    let invalid_leaf_queue_index = 0;
    nullify(
        &mut context,
        &merkle_tree_pubkey,
        &nullifier_queue_pubkey,
        &mut reference_merkle_tree,
        &elements[1].1,
        valid_changelog_index,
        invalid_leaf_queue_index,
        valid_element_index,
    )
    .await
    .unwrap_err();
    nullify(
        &mut context,
        &merkle_tree_pubkey,
        &nullifier_queue_pubkey,
        &mut reference_merkle_tree,
        &elements[1].1,
        valid_changelog_index,
        valid_leaf_queue_index,
        valid_element_index,
    )
    .await
    .unwrap();

    nullify(
        &mut context,
        &merkle_tree_pubkey,
        &invalid_nullifier_queue_pubkey,
        &mut reference_merkle_tree,
        &elements[0].1,
        2,
        0,
        element_index,
    )
    .await
    .unwrap_err();
}

async fn functional_2_test_insert_into_nullifier_queues<R: RpcConnection>(
    rpc: &mut R,
    nullifier_queue_pubkey: &Pubkey,
    merkle_tree_pubkey: &Pubkey,
) {
    let payer = rpc.get_payer().insecure_clone();
    let elements = vec![[1_u8; 32], [2_u8; 32]];
    insert_into_single_nullifier_queue(
        &elements,
        &payer,
        &payer,
        nullifier_queue_pubkey,
        merkle_tree_pubkey,
        rpc,
    )
    .await
    .unwrap();
    let array = unsafe { get_hash_set::<u16, QueueAccount, R>(rpc, *nullifier_queue_pubkey).await };
    let array_element_0 = array.by_value_index(0, None).unwrap();
    assert_eq!(array_element_0.value_bytes(), [1u8; 32]);
    assert_eq!(array_element_0.sequence_number(), None);
    let array_element_1 = array.by_value_index(1, None).unwrap();
    assert_eq!(array_element_1.value_bytes(), [2u8; 32]);
    assert_eq!(array_element_1.sequence_number(), None);
}

async fn fail_3_insert_same_elements_into_nullifier_queue<R: RpcConnection>(
    context: &mut R,
    nullifier_queue_pubkey: &Pubkey,
    merkle_tree_pubkey: &Pubkey,
    elements: Vec<[u8; 32]>,
) {
    let payer = context.get_payer().insecure_clone();

    insert_into_single_nullifier_queue(
        &elements,
        &payer,
        &payer,
        nullifier_queue_pubkey,
        merkle_tree_pubkey,
        context,
    )
    .await
    .unwrap_err();
}

async fn fail_4_insert_with_invalid_signer<R: RpcConnection>(
    rpc: &mut R,
    nullifier_queue_pubkey: &Pubkey,
    merkle_tree_pubkey: &Pubkey,
    elements: Vec<[u8; 32]>,
) {
    let invalid_signer = Keypair::new();
    airdrop_lamports(rpc, &invalid_signer.pubkey(), 1_000_000_000)
        .await
        .unwrap();
    insert_into_single_nullifier_queue(
        &elements,
        &invalid_signer,
        &invalid_signer,
        nullifier_queue_pubkey,
        merkle_tree_pubkey,
        rpc,
    )
    .await
    .unwrap_err();
}

async fn functional_5_test_insert_into_nullifier_queue<R: RpcConnection>(
    rpc: &mut R,
    nullifier_queue_pubkey: &Pubkey,
    merkle_tree_pubkey: &Pubkey,
) {
    let payer = rpc.get_payer().insecure_clone();
    let element = 3_u32.to_biguint().unwrap();
    let elements = vec![bigint_to_be_bytes_array(&element).unwrap()];
    insert_into_single_nullifier_queue(
        &elements,
        &payer,
        &payer,
        nullifier_queue_pubkey,
        merkle_tree_pubkey,
        rpc,
    )
    .await
    .unwrap();
    let array = unsafe { get_hash_set::<u16, QueueAccount, R>(rpc, *nullifier_queue_pubkey).await };

    let array_element = array.by_value_index(2, None).unwrap();
    assert_eq!(array_element.value_biguint(), element);
    assert_eq!(array_element.sequence_number(), None);
}

async fn insert_into_single_nullifier_queue<R: RpcConnection>(
    elements: &Vec<[u8; 32]>,
    fee_payer: &Keypair,
    payer: &Keypair,
    nullifier_queue_pubkey: &Pubkey,
    merkle_tree_pubkey: &Pubkey,
    context: &mut R,
) -> Result<(), RpcError> {
    let instruction_data = account_compression::instruction::InsertIntoNullifierQueues {
        elements: elements.to_vec(),
    };
    let accounts = account_compression::accounts::InsertIntoQueues {
        fee_payer: fee_payer.pubkey(),
        authority: payer.pubkey(),
        registered_program_pda: None,
        system_program: system_program::ID,
    };
    let mut remaining_accounts = Vec::with_capacity(elements.len() * 2);
    remaining_accounts.extend(
        vec![
            vec![
                AccountMeta::new(*nullifier_queue_pubkey, false),
                AccountMeta::new(*merkle_tree_pubkey, false)
            ];
            elements.len()
        ]
        .iter()
        .flat_map(|x| x.to_vec())
        .collect::<Vec<AccountMeta>>(),
    );
    let instruction = Instruction {
        program_id: ID,
        accounts: [accounts.to_account_metas(Some(true)), remaining_accounts].concat(),
        data: instruction_data.data(),
    };
    let latest_blockhash = context.get_latest_blockhash().await.unwrap();
    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&fee_payer.pubkey()),
        &vec![fee_payer, payer],
        latest_blockhash,
    );
    context.process_transaction(transaction.clone()).await
}

async fn insert_into_nullifier_queues<R: RpcConnection>(
    elements: &Vec<[u8; 32]>,
    fee_payer: &Keypair,
    payer: &Keypair,
    pubkeys: &[(Pubkey, Pubkey)],
    context: &mut R,
) -> Result<(), RpcError> {
    let instruction_data = account_compression::instruction::InsertIntoNullifierQueues {
        elements: elements.to_vec(),
    };
    let accounts = account_compression::accounts::InsertIntoQueues {
        fee_payer: fee_payer.pubkey(),
        authority: payer.pubkey(),
        registered_program_pda: None,
        system_program: system_program::ID,
    };
    let mut remaining_accounts = Vec::with_capacity(elements.len() * 2);
    for (nullifier_queue_pubkey, merkle_tree_pubkey) in pubkeys.iter() {
        remaining_accounts.push(AccountMeta::new(*nullifier_queue_pubkey, false));
        remaining_accounts.push(AccountMeta::new(*merkle_tree_pubkey, false));
    }
    let instruction = Instruction {
        program_id: ID,
        accounts: [accounts.to_account_metas(Some(true)), remaining_accounts].concat(),
        data: instruction_data.data(),
    };
    let latest_blockhash = context.get_latest_blockhash().await.unwrap();
    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&fee_payer.pubkey()),
        &vec![fee_payer, payer],
        latest_blockhash,
    );
    context.process_transaction(transaction.clone()).await
}

async fn functional_1_initialize_state_merkle_tree_and_nullifier_queue<R: RpcConnection>(
    rpc: &mut R,
    payer_pubkey: &Pubkey,
    merkle_tree_keypair: &Keypair,
    queue_keypair: &Keypair,
    network_fee: u64,
    rollover_threshold: Option<u64>,
    close_threshold: Option<u64>,
) -> Pubkey {
    let merkle_tree_account_create_ix = create_account_instruction(
        &rpc.get_payer().pubkey(),
        StateMerkleTreeAccount::LEN,
        rpc.get_minimum_balance_for_rent_exemption(
            account_compression::StateMerkleTreeAccount::LEN,
        )
        .await
        .unwrap(),
        &ID,
        Some(merkle_tree_keypair),
    );

    let size = QueueAccount::size(
        STATE_NULLIFIER_QUEUE_INDICES as usize,
        STATE_NULLIFIER_QUEUE_VALUES as usize,
    )
    .unwrap();
    let nullifier_queue_account_create_ix = create_account_instruction(
        payer_pubkey,
        size,
        rpc.get_minimum_balance_for_rent_exemption(size)
            .await
            .unwrap(),
        &ID,
        Some(queue_keypair),
    );
    let merkle_tree_pubkey = merkle_tree_keypair.pubkey();

    let state_merkle_tree_config = StateMerkleTreeConfig {
        rollover_threshold,
        close_threshold,
        network_fee: Some(network_fee),
        ..Default::default()
    };

    let instruction = create_initialize_merkle_tree_instruction(
        rpc.get_payer().pubkey(),
        merkle_tree_pubkey,
        queue_keypair.pubkey(),
        state_merkle_tree_config.clone(),
        NullifierQueueConfig::default(),
        None,
        1,
        0,
    );

    let latest_blockhash = rpc.get_latest_blockhash().await.unwrap();
    let transaction = Transaction::new_signed_with_payer(
        &[
            merkle_tree_account_create_ix,
            nullifier_queue_account_create_ix,
            instruction,
        ],
        Some(&rpc.get_payer().pubkey()),
        &vec![&rpc.get_payer(), &merkle_tree_keypair, queue_keypair],
        latest_blockhash,
    );
    rpc.process_transaction(transaction.clone()).await.unwrap();
    assert_merkle_tree_initialized(
        rpc,
        &merkle_tree_pubkey,
        &queue_keypair.pubkey(),
        STATE_MERKLE_TREE_HEIGHT as usize,
        STATE_MERKLE_TREE_CHANGELOG as usize,
        STATE_MERKLE_TREE_ROOTS as usize,
        STATE_MERKLE_TREE_CANOPY_DEPTH as usize,
        1,
        1,
        0,
        &Poseidon::zero_bytes()[0],
        rollover_threshold,
        close_threshold,
        network_fee,
        payer_pubkey,
    )
    .await;
    assert_nullifier_queue_initialized(
        rpc,
        &queue_keypair.pubkey(),
        &NullifierQueueConfig::default(),
        &merkle_tree_pubkey,
        &state_merkle_tree_config,
        QueueType::NullifierQueue,
        1,
        None,
        payer_pubkey,
    )
    .await;
    merkle_tree_pubkey
}

pub async fn fail_2_append_leaves_with_invalid_inputs<R: RpcConnection>(
    context: &mut R,
    merkle_tree_pubkeys: &[Pubkey],
    leaves: Vec<(u8, [u8; 32])>,
    expected_error: u32,
) -> Result<(), RpcError> {
    let instruction_data = account_compression::instruction::AppendLeavesToMerkleTrees {
        leaves, //: vec![(0, [1u8; 32]), (1, [2u8; 32])],
    };

    let accounts = account_compression::accounts::AppendLeaves {
        fee_payer: context.get_payer().pubkey(),
        authority: context.get_payer().pubkey(),
        registered_program_pda: None,
        log_wrapper: Pubkey::new_from_array(account_compression::utils::constants::NOOP_PUBKEY),
        system_program: system_program::ID,
    };

    let instruction = Instruction {
        program_id: ID,
        accounts: [
            accounts.to_account_metas(Some(true)),
            merkle_tree_pubkeys
                .iter()
                .map(|merkle_tree_pubkey| AccountMeta::new(*merkle_tree_pubkey, false))
                .collect::<Vec<AccountMeta>>(),
            // vec![AccountMeta::new(*merkle_tree_pubkey, false)],
        ]
        .concat(),
        data: instruction_data.data(),
    };

    let latest_blockhash = context.get_latest_blockhash().await.unwrap();
    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&context.get_payer().pubkey()),
        &vec![&context.get_payer()],
        latest_blockhash,
    );
    let result = context.process_transaction(transaction).await;
    assert_rpc_error(result, 0, expected_error)
}

pub async fn functional_3_append_leaves_to_merkle_tree<R: RpcConnection>(
    context: &mut R,
    reference_merkle_trees: &mut [&mut ConcurrentMerkleTree26<'_, Poseidon>],
    merkle_tree_pubkeys: &Vec<Pubkey>,
    leaves: &Vec<(u8, [u8; 32])>,
) {
    let payer = context.get_payer().insecure_clone();
    let mut hash_map = HashMap::<Pubkey, (Vec<[u8; 32]>, u64, usize, usize)>::new();
    for (i, leaf) in leaves {
        let pre_account_mt = context
            .get_account(merkle_tree_pubkeys[(*i) as usize])
            .await
            .unwrap()
            .unwrap();
        // pre_account_mts.push(pre_account_mt);
        let old_merkle_tree = AccountZeroCopy::<StateMerkleTreeAccount>::new(
            context,
            merkle_tree_pubkeys[(*i) as usize],
        )
        .await;
        let old_merkle_tree = old_merkle_tree.deserialized().copy_merkle_tree().unwrap();
        // old_merkle_trees.push(old_merkle_tree);
        hash_map
            .entry(merkle_tree_pubkeys[(*i) as usize])
            .or_insert_with(|| {
                (
                    Vec::<[u8; 32]>::new(),
                    pre_account_mt.lamports.clone(),
                    old_merkle_tree.next_index().clone(),
                    *i as usize,
                )
            })
            .0
            .push(leaf.clone());
    }

    let instruction = [create_insert_leaves_instruction(
        leaves.clone(),
        context.get_payer().pubkey(),
        context.get_payer().pubkey(),
        (*merkle_tree_pubkeys).clone(),
    )];

    context
        .create_and_send_transaction(&instruction, &payer.pubkey(), &[&payer, &payer])
        .await
        .unwrap();

    for (pubkey, (leaves, lamports, next_index, mt_index)) in hash_map.iter() {
        let num_leaves = leaves.len();
        let post_account_mt = context.get_account(*pubkey).await.unwrap().unwrap();
        let merkle_tree = AccountZeroCopy::<StateMerkleTreeAccount>::new(context, *pubkey).await;
        let merkle_tree_deserialized = merkle_tree.deserialized();
        let roll_over_fee = merkle_tree_deserialized
            .metadata
            .rollover_metadata
            .rollover_fee
            * (num_leaves as u64);
        let merkle_tree = merkle_tree_deserialized.copy_merkle_tree().unwrap();
        assert_eq!(merkle_tree.next_index, next_index + num_leaves as usize);
        let leaves: Vec<&[u8; 32]> = leaves.iter().map(|leaf| leaf).collect();
        (*reference_merkle_trees[*mt_index])
            .append_batch(&leaves)
            .unwrap();
        assert_eq!(merkle_tree.root(), reference_merkle_trees[*mt_index].root());
        assert_eq!(lamports + roll_over_fee, post_account_mt.lamports);
    }
}

pub async fn fail_4_append_leaves_with_invalid_authority<R: RpcConnection>(
    rpc: &mut R,
    merkle_tree_pubkey: &Pubkey,
) {
    let invalid_autority = Keypair::new();
    airdrop_lamports(rpc, &invalid_autority.pubkey(), 1_000_000_000)
        .await
        .unwrap();
    let instruction_data = account_compression::instruction::AppendLeavesToMerkleTrees {
        leaves: vec![(0, [1u8; 32])],
    };

    let accounts = account_compression::accounts::AppendLeaves {
        fee_payer: rpc.get_payer().pubkey(),
        authority: invalid_autority.pubkey(),
        registered_program_pda: None,
        log_wrapper: Pubkey::new_from_array(account_compression::utils::constants::NOOP_PUBKEY),
        system_program: system_program::ID,
    };

    let instruction = Instruction {
        program_id: ID,
        accounts: [
            accounts.to_account_metas(Some(true)),
            vec![AccountMeta::new(*merkle_tree_pubkey, false)],
        ]
        .concat(),
        data: instruction_data.data(),
    };
    let latest_blockhash = rpc.get_latest_blockhash().await.unwrap();
    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&invalid_autority.pubkey()),
        &vec![&rpc.get_payer(), &invalid_autority],
        latest_blockhash,
    );
    let remaining_accounts_mismatch_error = rpc.process_transaction(transaction).await;
    assert!(remaining_accounts_mismatch_error.is_err());
}

#[allow(clippy::too_many_arguments)]
pub async fn nullify<R: RpcConnection>(
    rpc: &mut R,
    merkle_tree_pubkey: &Pubkey,
    nullifier_queue_pubkey: &Pubkey,
    reference_merkle_tree: &mut MerkleTree<Poseidon>,
    element: &[u8; 32],
    change_log_index: u64,
    leaf_queue_index: u16,
    element_index: u64,
) -> Result<(), RpcError> {
    let payer = rpc.get_payer().insecure_clone();
    let proof: Vec<[u8; 32]> = reference_merkle_tree
        .get_proof_of_leaf(element_index as usize, false)
        .unwrap()
        .to_array::<16>()
        .unwrap()
        .to_vec();

    let instructions = [
        account_compression::nullify_leaves::sdk_nullify::create_nullify_instruction(
            vec![change_log_index].as_slice(),
            vec![leaf_queue_index].as_slice(),
            vec![element_index].as_slice(),
            vec![proof].as_slice(),
            &rpc.get_payer().pubkey(),
            merkle_tree_pubkey,
            nullifier_queue_pubkey,
        ),
    ];

    let event = rpc
        .create_and_send_transaction_with_event::<MerkleTreeEvent>(
            &instructions,
            &payer.pubkey(),
            &[&payer],
            None,
        )
        .await?;

    let merkle_tree =
        AccountZeroCopy::<StateMerkleTreeAccount>::new(rpc, *merkle_tree_pubkey).await;
    reference_merkle_tree
        .update(&ZERO_BYTES[0], element_index as usize)
        .unwrap();
    assert_eq!(
        merkle_tree
            .deserialized()
            .copy_merkle_tree()
            .unwrap()
            .root(),
        reference_merkle_tree.root()
    );

    let account = rpc
        .get_account(*nullifier_queue_pubkey)
        .await
        .unwrap()
        .unwrap();
    let mut data = account.data.clone();

    let nullifier_queue = &mut unsafe { queue_from_bytes_zero_copy_mut(&mut data).unwrap() };

    let array_element = nullifier_queue
        .by_value_index(
            leaf_queue_index.into(),
            Some(
                merkle_tree
                    .deserialized()
                    .copy_merkle_tree()
                    .unwrap()
                    .sequence_number,
            ),
        )
        .unwrap();
    assert_eq!(&array_element.value_bytes(), element);
    assert_eq!(
        array_element.sequence_number(),
        Some(
            merkle_tree
                .deserialized()
                .load_merkle_tree()
                .unwrap()
                .sequence_number
                + STATE_MERKLE_TREE_ROOTS as usize
        )
    );
    let event = event.as_ref().unwrap();
    match event {
        MerkleTreeEvent::V1(_) => panic!("Expected V2 event"),
        MerkleTreeEvent::V2(event_v1) => {
            assert_eq!(event_v1.id, merkle_tree_pubkey.to_bytes());
            assert_eq!(event_v1.nullified_leaves_indices[0], element_index);
        }
        MerkleTreeEvent::V3(_) => panic!("Expected V2 event"),
    }
    Ok(())
}

pub async fn set_nullifier_queue_to_full<R: RpcConnection>(
    rpc: &mut R,
    nullifier_queue_pubkey: &Pubkey,
    left_over_indices: usize,
    lamports: u64,
) {
    let mut account = rpc
        .get_account(*nullifier_queue_pubkey)
        .await
        .unwrap()
        .unwrap();
    let mut data = account.data.clone();
    let current_index;
    let capacity;
    {
        let hash_set = &mut unsafe { queue_from_bytes_zero_copy_mut(&mut data).unwrap() };
        current_index = unsafe { *hash_set.hash_set.next_value_index };
        println!("starting current index {}", current_index);
        capacity = hash_set.hash_set.capacity_values - left_over_indices;
        let arbitrary_sequence_number = 0;
        for i in current_index..capacity {
            hash_set
                .insert(&(i).to_biguint().unwrap(), arbitrary_sequence_number)
                .unwrap();
        }
    }
    assert_ne!(account.data, data);
    account.data = data;
    let mut account_share_data = AccountSharedData::from(account);
    account_share_data.set_lamports(lamports);
    rpc.set_account(nullifier_queue_pubkey, &account_share_data);
    let account = rpc
        .get_account(*nullifier_queue_pubkey)
        .await
        .unwrap()
        .unwrap();
    let mut data = account.data.clone();
    let nullifier_queue = &mut unsafe { queue_from_bytes_zero_copy_mut(&mut data).unwrap() };
    for i in current_index..capacity {
        let array_element = nullifier_queue.by_value_index(i, None).unwrap();
        assert_eq!(array_element.value_biguint(), i.to_biguint().unwrap());
    }
}

fn find_overlapping_probe_index(
    initial_value: usize,
    start_replacement_value: usize,
    capacity_values: usize,
) -> usize {
    for salt in 0..10000 {
        let replacement_value = start_replacement_value + salt;

        for i in 0..20 {
            let probe_index = (initial_value.clone()
                + i.to_biguint().unwrap() * i.to_biguint().unwrap())
                % capacity_values.to_biguint().unwrap();
            let replacement_probe_index = (replacement_value.clone()
                + i.to_biguint().unwrap() * i.to_biguint().unwrap())
                % capacity_values.to_biguint().unwrap();
            if probe_index == replacement_probe_index {
                return replacement_value;
            }
        }
    }
    panic!("No value with overlapping probe index found!");
}
async fn fail_insert_into_full_queue<R: RpcConnection>(
    context: &mut R,
    nullifier_queue_pubkey: &Pubkey,
    merkle_tree_pubkey: &Pubkey,
    elements: Vec<[u8; 32]>,
) {
    let payer = context.get_payer().insecure_clone();

    let result = insert_into_single_nullifier_queue(
        &elements,
        &payer,
        &payer,
        nullifier_queue_pubkey,
        merkle_tree_pubkey,
        context,
    )
    .await;

    assert_rpc_error(result, 0, HashSetError::Full.into()).unwrap();
}

pub async fn set_state_merkle_tree_sequence<R: RpcConnection>(
    rpc: &mut R,
    merkle_tree_pubkey: &Pubkey,
    sequence_number: u64,
    lamports: u64,
) {
    // is in range 9 - 10 in concurrent mt
    // offset for sequence number
    // let offset_start = 6 * 8 + 8 + 4 * 32 + 8 * 9;
    // let offset_end = offset_start + 8;
    let offset_start = 8
        + offset_of!(StateMerkleTreeAccount, state_merkle_tree_struct)
        + offset_of!(ConcurrentMerkleTree26<Poseidon>, sequence_number);
    let offset_end = offset_start + 8;
    let mut merkle_tree = rpc.get_account(*merkle_tree_pubkey).await.unwrap().unwrap();
    merkle_tree.data[offset_start..offset_end].copy_from_slice(&sequence_number.to_le_bytes());
    let mut account_share_data = AccountSharedData::from(merkle_tree);
    account_share_data.set_lamports(lamports);
    rpc.set_account(merkle_tree_pubkey, &account_share_data);
    let merkle_tree = rpc.get_account(*merkle_tree_pubkey).await.unwrap().unwrap();
    let data_in_offset = u64::from_le_bytes(
        merkle_tree.data[offset_start..offset_end]
            .try_into()
            .unwrap(),
    );
    assert_eq!(data_in_offset, sequence_number);
}

pub async fn assert_element_inserted_in_nullifier_queue_with_index(
    rpc: &mut ProgramTestRpcConnection,
    nullifier_queue_pubkey: &Pubkey,
    nullifier: [u8; 32],
    num_insertions: usize,
) {
    let array = unsafe {
        get_hash_set::<u16, QueueAccount, ProgramTestRpcConnection>(rpc, *nullifier_queue_pubkey)
            .await
    };
    let array_index = unsafe { (*array.next_value_index).clone() - num_insertions };
    let array_element = array.by_value_index(array_index, None).unwrap();
    assert_eq!(array_element.value_bytes(), nullifier);
    assert_eq!(array_element.sequence_number(), None);
}

async fn functional_6_test_insert_into_two_nullifier_queues(
    rpc: &mut ProgramTestRpcConnection,
    nullifiers: &Vec<[u8; 32]>,
    queue_tree_pairs: &[(Pubkey, Pubkey)],
) {
    let payer = rpc.get_payer().insecure_clone();
    insert_into_nullifier_queues(nullifiers, &payer, &payer, &queue_tree_pairs, rpc)
        .await
        .unwrap();
    assert_element_inserted_in_nullifier_queue_with_index(
        rpc,
        &queue_tree_pairs[0].0,
        nullifiers[0],
        1,
    )
    .await;
    assert_element_inserted_in_nullifier_queue_with_index(
        rpc,
        &queue_tree_pairs[1].0,
        nullifiers[1],
        1,
    )
    .await;
}

async fn functional_7_test_insert_into_two_nullifier_queues_not_ordered(
    rpc: &mut ProgramTestRpcConnection,
    nullifiers: &Vec<[u8; 32]>,
    queue_tree_pairs: &[(Pubkey, Pubkey)],
) {
    let payer = rpc.get_payer().insecure_clone();
    insert_into_nullifier_queues(nullifiers, &payer, &payer, &queue_tree_pairs, rpc)
        .await
        .unwrap();
    assert_element_inserted_in_nullifier_queue_with_index(
        rpc,
        &queue_tree_pairs[0].0,
        nullifiers[0],
        2,
    )
    .await;
    assert_element_inserted_in_nullifier_queue_with_index(
        rpc,
        &queue_tree_pairs[0].0,
        nullifiers[2],
        1,
    )
    .await;
    assert_element_inserted_in_nullifier_queue_with_index(
        rpc,
        &queue_tree_pairs[1].0,
        nullifiers[1],
        2,
    )
    .await;
    assert_element_inserted_in_nullifier_queue_with_index(
        rpc,
        &queue_tree_pairs[1].0,
        nullifiers[3],
        1,
    )
    .await;
}