use std::sync::Arc;

use nimiq_account::{BlockLogger, BlockState};
use nimiq_block::{ForkProof, MacroBlock, MacroBody, MacroHeader, SkipBlockInfo};
use nimiq_blockchain::{Blockchain, BlockchainConfig};
use nimiq_blockchain_interface::AbstractBlockchain;
use nimiq_database::{traits::WriteTransaction, volatile::VolatileDatabase};
use nimiq_hash::{Blake2bHash, Blake2sHash};
use nimiq_keys::Address;
use nimiq_primitives::{
    coin::Coin,
    networks::NetworkId,
    policy::Policy,
    slots_allocation::{JailedValidator, PenalizedSlot},
};
use nimiq_test_log::test;
use nimiq_test_utils::block_production::TemporaryBlockProducer;
use nimiq_transaction::inherent::Inherent;
use nimiq_utils::time::OffsetTime;
use nimiq_vrf::VrfSeed;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};

#[test]
fn it_can_create_batch_finalization_inherents() {
    let time = Arc::new(OffsetTime::new());
    let env = VolatileDatabase::new(20).unwrap();
    let blockchain = Arc::new(
        Blockchain::new(
            env,
            BlockchainConfig::default(),
            NetworkId::UnitAlbatross,
            time,
        )
        .unwrap(),
    );

    let macro_header = MacroHeader {
        version: 1,
        block_number: Policy::macro_block_of(2).unwrap(),
        round: 0,
        timestamp: blockchain.state().election_head.header.timestamp + 1,
        parent_hash: Blake2bHash::default(),
        parent_election_hash: Blake2bHash::default(),
        interlink: None,
        seed: VrfSeed::default(),
        extra_data: vec![],
        state_root: Blake2bHash::default(),
        body_root: Blake2sHash::default(),
        diff_root: Blake2bHash::default(),
        history_root: Blake2bHash::default(),
    };

    let staking_contract = blockchain.get_staking_contract();
    let active_validators = staking_contract.active_validators.clone();
    let reward_transactions =
        blockchain.create_reward_transactions(blockchain.state(), &macro_header, &staking_contract);

    let body = MacroBody {
        validators: None,
        next_batch_initial_punished_set: staking_contract
            .punished_slots
            .next_batch_initial_punished_set(macro_header.block_number, &active_validators),
        transactions: reward_transactions,
    };

    let macro_block = MacroBlock {
        header: macro_header.clone(),
        body: Some(body),
        justification: None,
    };

    // Simple case. Expect 1x FinalizeBatch, 1x Reward to validator
    let inherents = blockchain.finalize_previous_batch(&macro_block);
    assert_eq!(inherents.len(), 2);

    let (validator_address, _) = active_validators.iter().next().unwrap();

    let mut got_reward = false;
    let mut got_finalize_batch = false;
    for inherent in &inherents {
        match inherent {
            Inherent::Reward { value, .. } => {
                assert_eq!(*value, Coin::from_u64_unchecked(875));
                got_reward = true;
            }
            Inherent::FinalizeBatch => {
                got_finalize_batch = true;
            }
            _ => panic!(),
        }
    }
    assert!(got_reward && got_finalize_batch);

    // Penalize one slot. Expect 1x FinalizeBatch, 1x Reward to validator, 1x Reward burn
    let penalize_inherent = Inherent::Penalize {
        slot: PenalizedSlot {
            slot: 0,
            validator_address: validator_address.clone(),
            offense_event_block: 1 + Policy::genesis_block_number(),
        },
    };

    let mut txn = blockchain.write_transaction();
    // adds slot 0 to previous_lost_rewards -> slot won't get reward on next finalize_previous_batch
    assert!(blockchain
        .state()
        .accounts
        .commit(
            &mut (&mut txn).into(),
            &[],
            &[penalize_inherent],
            &BlockState::new(
                Policy::blocks_per_batch() + 1 + Policy::genesis_block_number(),
                1
            ),
            &mut BlockLogger::empty()
        )
        .is_ok());
    txn.commit();

    let staking_contract = blockchain.get_staking_contract();
    let reward_transactions =
        blockchain.create_reward_transactions(blockchain.state(), &macro_header, &staking_contract);
    let body = MacroBody {
        validators: None,
        next_batch_initial_punished_set: staking_contract
            .punished_slots
            .next_batch_initial_punished_set(macro_header.block_number, &active_validators),
        transactions: reward_transactions,
    };
    let macro_block = MacroBlock {
        header: macro_header,
        body: Some(body),
        justification: None,
    };

    let inherents = blockchain.finalize_previous_batch(&macro_block);
    assert_eq!(inherents.len(), 3);
    let one_slot_reward = 875 / Policy::SLOTS as u64;
    let mut got_reward = false;
    let mut got_penalize = false;
    let mut got_finalize_batch = false;

    for inherent in &inherents {
        match inherent {
            Inherent::Reward { target, value } => {
                if *target == Address::burn_address() {
                    assert_eq!(*value, Coin::from_u64_unchecked(one_slot_reward));
                    got_penalize = true;
                } else {
                    assert_eq!(
                        *value,
                        Coin::from_u64_unchecked(875 - one_slot_reward as u64)
                    );
                    got_reward = true;
                }
            }
            Inherent::FinalizeBatch => {
                got_finalize_batch = true;
            }
            _ => panic!(),
        }
    }
    assert!(got_reward && got_penalize && got_finalize_batch);
}

#[test]
fn it_can_penalize_delayed_batch() {
    let genesis_block_number = Policy::genesis_block_number();
    let time = Arc::new(OffsetTime::new());
    let env = VolatileDatabase::new(20).unwrap();
    let blockchain = Arc::new(
        Blockchain::new(
            env,
            BlockchainConfig::default(),
            NetworkId::UnitAlbatross,
            time,
        )
        .unwrap(),
    );

    // Delay in ms, so this means a 30s delay. For a 1m target batch time, this represents half of it
    let delay = 30000;

    let previous_timestamp = blockchain.state().election_head.header.timestamp;

    // We introduce a delay on purpose
    let next_timestamp = previous_timestamp
        + Policy::BLOCK_SEPARATION_TIME * (Policy::blocks_per_batch() as u64)
        + delay;

    let (genesis_supply, genesis_timestamp) = blockchain.get_genesis_parameters();

    // Total reward for the previous batch
    let prev_supply = Policy::supply_at(
        u64::from(genesis_supply),
        genesis_timestamp,
        genesis_timestamp,
    );

    let current_supply =
        Policy::supply_at(u64::from(genesis_supply), genesis_timestamp, next_timestamp);

    let max_reward = current_supply - prev_supply;

    let penalty = Policy::batch_delay_penalty(delay);

    log::info!(
        " The max available reward is {}, but due to a delay of {}ms there is a penalty of {}",
        max_reward,
        delay,
        penalty
    );

    let macro_header = MacroHeader {
        version: 1,
        block_number: 42 + genesis_block_number,
        round: 0,
        timestamp: next_timestamp,
        parent_hash: Blake2bHash::default(),
        parent_election_hash: Blake2bHash::default(),
        interlink: None,
        seed: VrfSeed::default(),
        extra_data: vec![],
        state_root: Blake2bHash::default(),
        body_root: Blake2sHash::default(),
        diff_root: Blake2bHash::default(),
        history_root: Blake2bHash::default(),
    };

    let staking_contract = blockchain.get_staking_contract();
    let reward_transactions =
        blockchain.create_reward_transactions(blockchain.state(), &macro_header, &staking_contract);

    let body = MacroBody {
        validators: None,
        next_batch_initial_punished_set: staking_contract
            .punished_slots
            .current_batch_punished_slots(),
        transactions: reward_transactions,
    };

    let macro_block = MacroBlock {
        header: macro_header,
        body: Some(body),
        justification: None,
    };

    // Simple case. Expect 1x FinalizeBatch, 1x Reward to validator
    let inherents = blockchain.finalize_previous_batch(&macro_block);
    assert_eq!(inherents.len(), 2);

    let mut got_reward = false;
    let mut got_finalize_batch = false;
    for inherent in &inherents {
        match inherent {
            Inherent::Reward { value, .. } => {
                assert_eq!(
                    *value,
                    Coin::from_u64_unchecked((max_reward as f64 * penalty) as u64)
                );
                got_reward = true;
            }
            Inherent::FinalizeBatch => {
                got_finalize_batch = true;
            }
            _ => panic!(),
        }
    }
    assert!(got_reward && got_finalize_batch);
}

#[test]
/// Create a skip block and check that correct inherents are produced.
fn it_correctly_creates_inherents_from_skip_block() {
    let temp_producer1 = TemporaryBlockProducer::new();
    let skip_block = temp_producer1.next_block(vec![], true);
    let skip_block = skip_block.unwrap_micro();

    let blockchain_rg = temp_producer1.blockchain.read();
    let (validator, slot) = blockchain_rg
        .get_slot_owner_at(skip_block.block_number(), skip_block.block_number(), None)
        .unwrap();

    let skip_block_info = SkipBlockInfo::from_micro_block(&skip_block);

    // Create the inherents from any forks or skip block info.
    let inherents = blockchain_rg.create_punishment_inherents(
        skip_block.block_number(),
        &skip_block.body.as_ref().unwrap().fork_proofs,
        skip_block_info,
        None,
    );

    // Check inherents are correct.
    assert_eq!(
        inherents,
        vec![Inherent::Penalize {
            slot: PenalizedSlot {
                slot,
                validator_address: validator.address,
                offense_event_block: skip_block.block_number()
            }
        }]
    );
}

#[test]
/// Create a block with fork proof and check that correct inherents are produced.
fn it_correctly_creates_inherents_from_fork_proof() {
    let temp_producer1 = TemporaryBlockProducer::new();
    // Create block 1 of the fork (which is not pushed to the blockchain).
    let micro_block_fork1 = temp_producer1.next_block_no_push(vec![], false);
    let micro_block_fork1 = micro_block_fork1.unwrap_micro();

    // Create block 2 of the fork (which *is* pushed to the blockchain).
    let micro_block_fork2 = temp_producer1.next_block(vec![0x42], false);
    let micro_block_fork2 = micro_block_fork2.unwrap_micro();

    // Create a follow up block, which will contain the fork proof.
    let reporting_micro_block = temp_producer1.next_block(vec![], false);
    let mut reporting_micro_block = reporting_micro_block.unwrap_micro();

    // Produce and add the fork proof.
    let fork_proof = ForkProof {
        header1: micro_block_fork1.header.clone(),
        header2: micro_block_fork2.header.clone(),
        justification1: micro_block_fork1
            .justification
            .clone()
            .unwrap()
            .unwrap_micro(),
        justification2: micro_block_fork2
            .justification
            .clone()
            .unwrap()
            .unwrap_micro(),
        prev_vrf_seed: micro_block_fork2.header.seed.clone(),
    };
    reporting_micro_block
        .body
        .as_mut()
        .unwrap()
        .fork_proofs
        .push(fork_proof);

    let blockchain_rg = temp_producer1.blockchain.read();
    let (validator, _slot) = blockchain_rg
        .get_slot_owner_at(
            micro_block_fork1.block_number(),
            micro_block_fork1.block_number(),
            None,
        )
        .unwrap();

    let skip_block_info = SkipBlockInfo::from_micro_block(&reporting_micro_block);

    // Create the inherents from any forks or skip block info.
    let inherents = blockchain_rg.create_punishment_inherents(
        reporting_micro_block.block_number(),
        &reporting_micro_block.body.unwrap().fork_proofs,
        skip_block_info,
        None,
    );

    // Check inherents are correct.
    assert_eq!(
        inherents,
        vec![Inherent::Jail {
            jailed_validator: JailedValidator {
                slots: validator.slots,
                validator_address: validator.address,
                offense_event_block: micro_block_fork1.block_number(),
            },
            new_epoch_slot_range: None
        }]
    );
}

#[test]
/// Create a block with fork proof in the following epoch and check that correct inherents are produced.
fn it_correctly_creates_inherents_in_next_epoch_from_fork_proof() {
    let temp_producer1 = TemporaryBlockProducer::new();
    // Fill the blockchain with enough blocks to be in the last batch of the first epoch.
    for _ in 0..Policy::blocks_per_epoch() - 2 {
        temp_producer1.next_block(vec![], false);
    }

    // Create block 1 of the fork (which is not pushed to the blockchain).
    let micro_block_fork1 = temp_producer1.next_block_no_push(vec![], false);
    let micro_block_fork1 = micro_block_fork1.unwrap_micro();

    // Create block 2 of the fork (which *is* pushed to the blockchain).
    let micro_block_fork2 = temp_producer1.next_block(vec![0x42], false);
    let micro_block_fork2 = micro_block_fork2.unwrap_micro();

    // Create macro block.
    temp_producer1.next_block(vec![], false);

    // Create a follow up block in the next epoch, which will contain the fork proof.
    let reporting_micro_block = temp_producer1.next_block(vec![], false);
    let mut reporting_micro_block = reporting_micro_block.unwrap_micro();

    assert_ne!(
        Policy::epoch_at(micro_block_fork1.block_number()),
        Policy::epoch_at(reporting_micro_block.block_number())
    );

    // Produce and add the fork proof.
    let fork_proof = ForkProof {
        header1: micro_block_fork1.header.clone(),
        header2: micro_block_fork2.header.clone(),
        justification1: micro_block_fork1
            .justification
            .clone()
            .unwrap()
            .unwrap_micro(),
        justification2: micro_block_fork2
            .justification
            .clone()
            .unwrap()
            .unwrap_micro(),
        prev_vrf_seed: micro_block_fork2.header.seed.clone(),
    };
    reporting_micro_block
        .body
        .as_mut()
        .unwrap()
        .fork_proofs
        .push(fork_proof);

    let blockchain_rg = temp_producer1.blockchain.read();
    let (validator, _slot) = blockchain_rg
        .get_slot_owner_at(
            micro_block_fork1.block_number(),
            micro_block_fork1.block_number(),
            None,
        )
        .unwrap();
    let current_epoch_validator = blockchain_rg
        .current_validators()
        .expect("We need to have validators")
        .get_validator_by_address(&validator.address)
        .unwrap()
        .clone();

    let skip_block_info = SkipBlockInfo::from_micro_block(&reporting_micro_block);

    // Create the inherents from any forks or skip block info.
    let inherents = blockchain_rg.create_punishment_inherents(
        reporting_micro_block.block_number(),
        &reporting_micro_block.body.unwrap().fork_proofs,
        skip_block_info,
        None,
    );

    // Check inherents are correct.
    assert_eq!(
        inherents,
        vec![Inherent::Jail {
            jailed_validator: JailedValidator {
                slots: validator.slots,
                validator_address: validator.address,
                offense_event_block: micro_block_fork1.block_number(),
            },
            new_epoch_slot_range: Some(current_epoch_validator.slots)
        }]
    );
}

#[test(tokio::test)]
async fn create_fork_proof() {
    // Build a fork using two producers.
    let producer1 = TemporaryBlockProducer::new();
    let producer2 = TemporaryBlockProducer::new();

    let mut fork_rx = BroadcastStream::new(producer1.blockchain.read().fork_notifier.subscribe());

    // Easy rebranch
    // [0] - [0] - [0] - [0]
    //          \- [0]
    let block = producer1.next_block(vec![], false);
    let _next_block = producer1.next_block(vec![0x48], false);
    producer2.push(block).unwrap();

    let fork = producer2.next_block(vec![], false);
    producer1.push(fork).unwrap();

    // Verify that the fork proof was generated
    assert!(fork_rx.next().await.is_some());
}
