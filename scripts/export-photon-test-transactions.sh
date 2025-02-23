#!/bin/bash

# Note generated data hasn't been in photon tests yet.
# expected test results, 50 compressed accounts with 1_000_000 each owned by Pubkey::new_unique() (produces pubkeys deterministicly)
# fully forested
cargo test -p forester -- --test test_state_batched;
cargo xtask export-photon-test-data --test-name test_state_batched;
killall solana-test-validator;

cargo test-sbf -p compressed-token-test -- --test test_transfer_with_photon_and_batched_tree;
cargo xtask export-photon-test-data --test-name test_batched_token;
killall solana-test-validator;
