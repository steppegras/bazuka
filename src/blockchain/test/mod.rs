use super::*;
use crate::config::blockchain;
use crate::core::{Hasher, Signature, Signer, TransactionData};
use crate::crypto::SignatureScheme;
use crate::db;

mod contract;
mod tokens;

fn easy_config() -> BlockchainConfig {
    let mut conf = blockchain::get_test_blockchain_config();
    conf.genesis.block.header.proof_of_work.target = Difficulty(0x00ffffff);
    conf.minimum_pow_difficulty = Difficulty(0x00ffffff);

    conf
}

fn rollback_till_empty<K: KvStore>(b: &mut KvStoreChain<K>) -> Result<(), BlockchainError> {
    while b.get_height()? > 0 {
        assert_eq!(circulated_money(b)?, Amount(2000000000000000000));
        b.rollback()?;
    }
    assert_eq!(circulated_money(b)?, Amount(0));
    assert!(matches!(
        b.rollback(),
        Err(BlockchainError::NoBlocksToRollback)
    ));
    assert!(b.database.pairs("".into())?.is_empty());
    Ok(())
}

fn circulated_money<K: KvStore>(b: &KvStoreChain<K>) -> Result<Amount, BlockchainError> {
    let mut money_sum = Amount(0);
    for (k, v) in b.database.pairs("ACB-".into())? {
        if k.0.ends_with("Ziesha") {
            let bal: Amount = v.try_into().unwrap();
            money_sum += bal;
        }
    }
    for (k, v) in b.database.pairs("CAB-".into())? {
        if k.0.ends_with("Ziesha") {
            let bal: Amount = v.try_into().unwrap();
            money_sum += bal;
        }
    }
    Ok(money_sum)
}

#[test]
fn test_get_header_and_get_block() -> Result<(), BlockchainError> {
    let miner = TxBuilder::new(&Vec::from("MINER"));
    let mut chain = KvStoreChain::new(db::RamKvStore::new(), easy_config())?;

    let new_block = chain.draft_block(60, &[], &miner, true)?.unwrap().block;
    chain.extend(1, &[new_block.clone()])?;

    assert_eq!(chain.get_block(1)?, new_block);
    assert_eq!(chain.get_header(1)?, new_block.header);

    assert!(matches!(
        chain.get_block(2),
        Err(BlockchainError::BlockNotFound)
    ));
    assert!(matches!(
        chain.get_header(2),
        Err(BlockchainError::BlockNotFound)
    ));

    let mut broken_chain = chain.fork_on_ram();

    broken_chain
        .database
        .update(&vec![WriteOp::Put("HGT".into(), 3u64.into())])?;

    assert!(matches!(
        broken_chain.get_block(2),
        Err(BlockchainError::Inconsistency)
    ));
    assert!(matches!(
        broken_chain.get_header(2),
        Err(BlockchainError::Inconsistency)
    ));

    assert!(matches!(
        rollback_till_empty(&mut broken_chain),
        Err(BlockchainError::Inconsistency)
    ));
    rollback_till_empty(&mut chain)?;

    Ok(())
}

#[test]
fn test_correct_target_calculation() -> Result<(), BlockchainError> {
    let miner = TxBuilder::new(&Vec::from("MINER"));
    let mut chain = KvStoreChain::new(db::RamKvStore::new(), easy_config())?;

    chain.apply_block(
        &chain.draft_block(60, &[], &miner, true)?.unwrap().block,
        true,
    )?;

    let mut wrong_pow = chain.draft_block(120, &[], &miner, true)?.unwrap();
    wrong_pow.block.header.proof_of_work.target = Difficulty(0x01ffffff);
    assert!(matches!(
        chain.apply_block(&wrong_pow.block, true),
        Err(BlockchainError::DifficultyTargetWrong)
    ));

    // TODO: Add more blocks to check correct difficulty recalculation

    Ok(())
}

#[test]
fn test_difficulty_target_recalculation() -> Result<(), BlockchainError> {
    let miner = TxBuilder::new(&Vec::from("MINER"));
    let mut conf = easy_config();
    conf.minimum_pow_difficulty = Difficulty::from_power(20);
    conf.block_time = 60;
    conf.difficulty_window = 2;
    conf.difficulty_cut = 0;
    conf.difficulty_lag = 0;
    let mut chain = KvStoreChain::new(db::RamKvStore::new(), conf.clone())?;

    let mut draft = chain.draft_block(30, &[], &miner, true)?.unwrap();
    assert_eq!(draft.block.header.proof_of_work.target.power(), 20);
    mine_block(&chain, &mut draft)?;
    chain.extend(1, &[draft.block])?;

    draft = chain.draft_block(60, &[], &miner, true)?.unwrap();
    assert_eq!(draft.block.header.proof_of_work.target.power(), 40);
    mine_block(&chain, &mut draft)?;
    chain.extend(2, &[draft.block])?;

    draft = chain.draft_block(120, &[], &miner, true)?.unwrap();
    assert_eq!(draft.block.header.proof_of_work.target.power(), 80);
    mine_block(&chain, &mut draft)?;
    chain.extend(3, &[draft.block])?;

    draft = chain.draft_block(480, &[], &miner, true)?.unwrap();
    mine_block(&chain, &mut draft)?;
    assert_eq!(draft.block.header.proof_of_work.target.power(), 80);
    chain.extend(4, &[draft.block])?;

    draft = chain.draft_block(540, &[], &miner, true)?.unwrap();
    assert_eq!(draft.block.header.proof_of_work.target.power(), 20);
    mine_block(&chain, &mut draft)?;
    chain.extend(5, &[draft.block])?;

    draft = chain.draft_block(590, &[], &miner, true)?.unwrap();
    assert_eq!(draft.block.header.proof_of_work.target.power(), 20);
    mine_block(&chain, &mut draft)?;
    chain.extend(6, &[draft.block])?;

    draft = chain.draft_block(610, &[], &miner, true)?.unwrap();
    assert_eq!(draft.block.header.proof_of_work.target.power(), 24);
    mine_block(&chain, &mut draft)?;
    chain.extend(7, &[draft.block])?;

    draft = chain.draft_block(650, &[], &miner, true)?.unwrap();
    assert_eq!(draft.block.header.proof_of_work.target.power(), 72);
    mine_block(&chain, &mut draft)?;
    chain.extend(8, &[draft.block])?;

    draft = chain.draft_block(900, &[], &miner, true)?.unwrap();
    assert_eq!(draft.block.header.proof_of_work.target.power(), 108);
    mine_block(&chain, &mut draft)?;
    chain.extend(9, &[draft.block])?;

    draft = chain.draft_block(1000, &[], &miner, true)?.unwrap();
    assert_eq!(draft.block.header.proof_of_work.target.power(), 25);
    mine_block(&chain, &mut draft)?;
    chain.extend(10, &[draft.block])?;

    draft = chain.draft_block(2000, &[], &miner, true)?.unwrap();
    assert_eq!(draft.block.header.proof_of_work.target.power(), 20);
    mine_block(&chain, &mut draft)?;
    chain.extend(11, &[draft.block])?;

    draft = chain.draft_block(3000, &[], &miner, true)?.unwrap();
    assert_eq!(draft.block.header.proof_of_work.target.power(), 20);
    mine_block(&chain, &mut draft)?;
    chain.extend(12, &[draft.block])?;

    // TODO: Check difficulty overflow (One can't make 0x00ffffff easier)

    let chain2 = KvStoreChain::new(db::RamKvStore::new(), conf)?;
    let headers = chain.get_headers(1, 100)?;
    assert!(chain2.will_extend(1, &headers, true)?);

    for i in 0..headers.len() {
        let mut broken_headers = headers.clone();
        broken_headers[i].proof_of_work.target = Difficulty(0x00aabbcc);
        assert!(matches!(
            chain2.will_extend(1, &broken_headers, true),
            Err(BlockchainError::DifficultyTargetWrong)
        ));
    }

    rollback_till_empty(&mut chain)?;

    Ok(())
}

#[test]
fn test_pow_key_correctness() -> Result<(), BlockchainError> {
    let miner = TxBuilder::new(&Vec::from("MINER"));
    let mut conf = easy_config();
    conf.pow_key_change_delay = 4;
    conf.pow_key_change_interval = 8;
    let mut chain = KvStoreChain::new(db::RamKvStore::new(), conf)?;

    for i in 0..25 {
        let mut draft = chain.draft_block(i * 60, &[], &miner, true)?.unwrap();
        mine_block(&chain, &mut draft)?;
        chain.apply_block(&draft.block, true)?;
        chain.update_states(&draft.patch)?;
        let pow_key = chain.pow_key(i as u64)?;
        if i < 4 {
            assert_eq!(
                pow_key,
                vec![66, 65, 90, 85, 75, 65, 32, 66, 65, 83, 69, 32, 75, 69, 89]
            );
        } else if i < 12 {
            let block0_hash = chain.get_block(0)?.header.hash();
            assert_eq!(pow_key, block0_hash);
        } else if i < 20 {
            let block8_hash = chain.get_block(8)?.header.hash();
            assert_eq!(pow_key, block8_hash);
        } else {
            let block16_hash = chain.get_block(16)?.header.hash();
            assert_eq!(pow_key, block16_hash);
        }
    }

    rollback_till_empty(&mut chain)?;

    Ok(())
}

#[test]
fn test_median_timestamp_correctness_check() -> Result<(), BlockchainError> {
    let miner = TxBuilder::new(&Vec::from("MINER"));
    let mut chain = KvStoreChain::new(db::RamKvStore::new(), easy_config())?;

    let mut fork1 = chain.fork_on_ram();
    fork1.apply_block(
        &fork1.draft_block(10, &[], &miner, true)?.unwrap().block,
        false,
    )?;
    assert!(matches!(
        fork1.draft_block(
            5, // 5 < 10
            &[],
            &miner,
            true,
        ),
        Err(BlockchainError::InvalidTimestamp)
    ));
    fork1.apply_block(
        &fork1
            .draft_block(
                10, // 10, again, should be fine
                &[],
                &miner,
                true,
            )?
            .unwrap()
            .block,
        false,
    )?;

    for i in 11..30 {
        fork1.apply_block(
            &fork1.draft_block(i, &[], &miner, true)?.unwrap().block,
            false,
        )?;
    }

    // 10 last timestamps are: 29 28 27 26 25 24 23 22 21 20
    // Median is: 25
    // 24 should fail. 25 should be fine.
    assert!(matches!(
        fork1.draft_block(
            24, // 24 < 25
            &[],
            &miner,
            true,
        ),
        Err(BlockchainError::InvalidTimestamp)
    ));
    fork1.apply_block(
        &fork1.draft_block(25, &[], &miner, true)?.unwrap().block,
        false,
    )?;

    rollback_till_empty(&mut fork1)?;
    drop(fork1);
    rollback_till_empty(&mut chain)?;

    Ok(())
}

#[test]
fn test_block_number_correctness_check() -> Result<(), BlockchainError> {
    let miner = TxBuilder::new(&Vec::from("MINER"));
    let mut chain = KvStoreChain::new(db::RamKvStore::new(), easy_config())?;
    let mut fork1 = chain.fork_on_ram();
    let blk1 = fork1.draft_block(0, &[], &miner, true)?.unwrap();
    fork1.extend(1, &[blk1.block.clone()])?;
    let blk2 = fork1.draft_block(1, &[], &miner, true)?.unwrap();
    fork1.extend(2, &[blk2.block.clone()])?;
    assert_eq!(fork1.get_height()?, 3);

    let mut fork2 = chain.fork_on_ram();
    fork2.extend(1, &[blk1.block.clone(), blk2.block.clone()])?;
    assert_eq!(fork2.get_height()?, 3);

    let mut fork3 = chain.fork_on_ram();
    let mut blk1_wrong_num = blk1.clone();
    blk1_wrong_num.block.header.number += 1;
    assert!(matches!(
        fork3.extend(1, &[blk1_wrong_num.block, blk2.block.clone()]),
        Err(BlockchainError::InvalidBlockNumber)
    ));

    let mut fork4 = chain.fork_on_ram();
    let mut blk2_wrong_num = blk2.clone();
    blk2_wrong_num.block.header.number += 1;
    assert!(matches!(
        fork4.extend(1, &[blk1.block, blk2_wrong_num.block.clone()]),
        Err(BlockchainError::InvalidBlockNumber)
    ));

    rollback_till_empty(&mut fork1)?;
    rollback_till_empty(&mut fork2)?;
    rollback_till_empty(&mut fork3)?;
    rollback_till_empty(&mut fork4)?;
    drop((fork1, fork2, fork3, fork4));
    rollback_till_empty(&mut chain)?;

    Ok(())
}

#[test]
fn test_parent_hash_correctness_check() -> Result<(), BlockchainError> {
    let miner = TxBuilder::new(&Vec::from("MINER"));
    let mut chain = KvStoreChain::new(db::RamKvStore::new(), easy_config())?;
    let mut fork1 = chain.fork_on_ram();
    let blk1 = fork1.draft_block(0, &[], &miner, true)?.unwrap();
    fork1.extend(1, &[blk1.block.clone()])?;
    let blk2 = fork1.draft_block(1, &[], &miner, true)?.unwrap();
    fork1.extend(2, &[blk2.block.clone()])?;
    assert_eq!(fork1.get_height()?, 3);

    let mut fork2 = chain.fork_on_ram();
    fork2.extend(1, &[blk1.block.clone(), blk2.block.clone()])?;
    assert_eq!(fork2.get_height()?, 3);

    let mut fork3 = chain.fork_on_ram();
    let mut blk1_wrong = blk1.clone();
    blk1_wrong.block.header.parent_hash = Default::default();
    assert!(matches!(
        fork3.extend(1, &[blk1_wrong.block, blk2.block.clone()]),
        Err(BlockchainError::InvalidParentHash)
    ));

    let mut fork4 = chain.fork_on_ram();
    let mut blk2_wrong = blk2.clone();
    blk2_wrong.block.header.parent_hash = Default::default();
    assert!(matches!(
        fork4.extend(1, &[blk1.block, blk2_wrong.block.clone()]),
        Err(BlockchainError::InvalidParentHash)
    ));

    rollback_till_empty(&mut fork1)?;
    rollback_till_empty(&mut fork2)?;
    rollback_till_empty(&mut fork3)?;
    rollback_till_empty(&mut fork4)?;
    drop((fork1, fork2, fork3, fork4));
    rollback_till_empty(&mut chain)?;

    Ok(())
}

#[test]
fn test_merkle_root_check() -> Result<(), BlockchainError> {
    let alice = TxBuilder::new(&Vec::from("ABC"));
    let miner = TxBuilder::new(&Vec::from("MINER"));
    let mut chain = KvStoreChain::new(db::RamKvStore::new(), easy_config())?;
    let blk1 = chain
        .draft_block(
            1,
            &[
                alice.create_transaction(
                    "".into(),
                    miner.get_address(),
                    Money::ziesha(100),
                    Money::ziesha(0),
                    1,
                ),
                alice.create_transaction(
                    "".into(),
                    miner.get_address(),
                    Money::ziesha(200),
                    Money::ziesha(0),
                    2,
                ),
            ],
            &miner,
            true,
        )?
        .unwrap()
        .block;
    let blk2 = chain
        .draft_block(
            1,
            &[
                alice.create_transaction(
                    "".into(),
                    miner.get_address(),
                    Money::ziesha(200),
                    Money::ziesha(0),
                    1,
                ),
                alice.create_transaction(
                    "".into(),
                    miner.get_address(),
                    Money::ziesha(100),
                    Money::ziesha(0),
                    2,
                ),
            ],
            &miner,
            true,
        )?
        .unwrap()
        .block;

    let mut fork1 = chain.fork_on_ram();
    let mut fork2 = chain.fork_on_ram();

    fork1.apply_block(&blk1, true)?;
    assert_eq!(
        fork1.get_balance(alice.get_address(), TokenId::Ziesha)?,
        Amount(9700)
    );

    fork2.apply_block(&blk2, true)?;
    assert_eq!(
        fork2.get_balance(alice.get_address(), TokenId::Ziesha)?,
        Amount(9700)
    );

    let mut blk_wrong = blk1.clone();
    blk_wrong.header.block_root = Default::default();
    assert!(matches!(
        chain.fork_on_ram().apply_block(&blk_wrong, true),
        Err(BlockchainError::InvalidMerkleRoot)
    ));

    rollback_till_empty(&mut fork1)?;
    rollback_till_empty(&mut fork2)?;
    drop((fork1, fork2));
    rollback_till_empty(&mut chain)?;

    Ok(())
}

#[test]
fn test_txs_cant_be_duplicated() -> Result<(), BlockchainError> {
    let miner = TxBuilder::new(&Vec::from("MINER"));
    let alice = TxBuilder::new(&Vec::from("ABC"));
    let bob = TxBuilder::new(&Vec::from("CBA"));

    let mut chain = KvStoreChain::new(db::RamKvStore::new(), easy_config())?;

    // Alice: 10000 Bob: 0
    assert_eq!(
        chain.get_balance(alice.get_address(), TokenId::Ziesha)?,
        Amount(10000)
    );
    assert_eq!(
        chain.get_balance(bob.get_address(), TokenId::Ziesha)?,
        Amount(0)
    );

    let tx = alice.create_transaction(
        "".into(),
        bob.get_address(),
        Money::ziesha(2700),
        Money::ziesha(300),
        1,
    );

    // Alice -> 2700 -> Bob (Fee 300)
    chain.apply_block(
        &chain
            .draft_block(1, &[tx.clone()], &miner, true)?
            .unwrap()
            .block,
        true,
    )?;
    assert_eq!(
        chain.get_balance(alice.get_address(), TokenId::Ziesha)?,
        Amount(7000)
    );
    assert_eq!(
        chain.get_balance(bob.get_address(), TokenId::Ziesha)?,
        Amount(2700)
    );

    // Alice -> 2700 -> Bob (Fee 300) (NOT APPLIED: DUPLICATED TRANSACTION!)
    chain.apply_block(
        &chain
            .draft_block(1, &[tx.clone()], &miner, true)?
            .unwrap()
            .block,
        true,
    )?;
    assert_eq!(
        chain.get_balance(alice.get_address(), TokenId::Ziesha)?,
        Amount(7000)
    );
    assert_eq!(
        chain.get_balance(bob.get_address(), TokenId::Ziesha)?,
        Amount(2700)
    );

    let tx2 = alice.create_transaction(
        "".into(),
        bob.get_address(),
        Money::ziesha(2700),
        Money::ziesha(300),
        2,
    );

    // Alice -> 2700 -> Bob (Fee 300)
    chain.apply_block(
        &chain.draft_block(1, &[tx2], &miner, true)?.unwrap().block,
        true,
    )?;
    assert_eq!(
        chain.get_balance(alice.get_address(), TokenId::Ziesha)?,
        Amount(4000)
    );
    assert_eq!(
        chain.get_balance(bob.get_address(), TokenId::Ziesha)?,
        Amount(5400)
    );

    rollback_till_empty(&mut chain)?;

    Ok(())
}

#[test]
fn test_insufficient_balance_is_handled() -> Result<(), BlockchainError> {
    let miner = TxBuilder::new(&Vec::from("MINER"));
    let alice = TxBuilder::new(&Vec::from("ABC"));
    let bob = TxBuilder::new(&Vec::from("CBA"));

    let mut chain = KvStoreChain::new(db::RamKvStore::new(), easy_config())?;

    // Alice: 10000 Bob: 0
    assert_eq!(
        chain.get_balance(alice.get_address(), TokenId::Ziesha)?,
        Amount(10000)
    );
    assert_eq!(
        chain.get_balance(bob.get_address(), TokenId::Ziesha)?,
        Amount(0)
    );

    let tx = alice.create_transaction(
        "".into(),
        bob.get_address(),
        Money::ziesha(9701),
        Money::ziesha(300),
        1,
    );

    // Ensure apply_tx will raise
    match chain.apply_tx(&tx.tx, false) {
        Ok(_) => assert!(
            false,
            "Transaction from wallet with insufficient fund should fail"
        ),
        Err(e) => assert!(matches!(e, BlockchainError::BalanceInsufficient)),
    }

    // Ensure tx is not included in block and bob has not received funds
    chain.apply_block(
        &chain.draft_block(1, &[tx], &miner, true)?.unwrap().block,
        true,
    )?;
    assert_eq!(
        chain.get_balance(bob.get_address(), TokenId::Ziesha)?,
        Amount(0)
    );

    rollback_till_empty(&mut chain)?;

    Ok(())
}

#[test]
fn test_cant_apply_unsigned_tx() -> Result<(), BlockchainError> {
    let miner = TxBuilder::new(&Vec::from("MINER"));
    let alice = TxBuilder::new(&Vec::from("ABC"));
    let bob = TxBuilder::new(&Vec::from("CBA"));

    let mut chain = KvStoreChain::new(db::RamKvStore::new(), easy_config())?;

    // Create unsigned signed tx
    let unsigned_tx = Transaction {
        memo: "".into(),
        src: Some(alice.get_address()),
        data: TransactionData::RegularSend {
            entries: vec![RegularSendEntry {
                dst: bob.get_address(),
                amount: Money::ziesha(1000),
            }],
        },
        nonce: 1,
        fee: Money::ziesha(300),
        sig: Signature::Unsigned,
    };
    let unsigned_tx = TransactionAndDelta {
        tx: unsigned_tx,
        state_delta: None,
    };

    // Ensure apply_tx will raise
    match chain.draft_block(1, &[unsigned_tx.clone()], &miner, false) {
        Ok(_) => assert!(false, "Unsigned transaction shall not be applied"),
        Err(e) => assert!(matches!(e, BlockchainError::SignatureError)),
    }

    // Ensure tx is not included in block and bob has not received funds
    chain.apply_block(
        &chain
            .draft_block(1, &[unsigned_tx], &miner, true)?
            .unwrap()
            .block,
        true,
    )?;
    assert_eq!(
        chain.get_balance(bob.get_address(), TokenId::Ziesha)?,
        Amount(0)
    );

    rollback_till_empty(&mut chain)?;

    Ok(())
}

#[test]
fn test_cant_apply_invalid_signed_tx() -> Result<(), BlockchainError> {
    let miner = TxBuilder::new(&Vec::from("MINER"));
    let alice = TxBuilder::new(&Vec::from("ABC"));
    let bob = TxBuilder::new(&Vec::from("CBA"));

    let mut chain = KvStoreChain::new(db::RamKvStore::new(), easy_config())?;

    // Create unsigned tx
    let (_, sk) = Signer::generate_keys(&Vec::from("ABC"));
    let mut tx = Transaction {
        memo: "".into(),
        src: Some(alice.get_address()),
        data: TransactionData::RegularSend {
            entries: vec![RegularSendEntry {
                dst: bob.get_address(),
                amount: Money::ziesha(1000),
            }],
        },
        nonce: 1,
        fee: Money::ziesha(300),
        sig: Signature::Unsigned,
    };

    let mut bytes = bincode::serialize(&tx).unwrap();
    bytes.push(0x11);

    tx.sig = Signature::Signed(Signer::sign(&sk, &bytes));
    let tx = TransactionAndDelta {
        tx,
        state_delta: None,
    };

    // Ensure apply_tx will raise
    match chain.draft_block(1, &[tx.clone()], &miner, false) {
        Ok(_) => assert!(false, "Invalid signed transaction shall not be applied"),
        Err(e) => assert!(matches!(e, BlockchainError::SignatureError)),
    }

    // Ensure tx is not included in block and bob has not received funds
    chain.apply_block(
        &chain.draft_block(1, &[tx], &miner, true)?.unwrap().block,
        true,
    )?;
    assert_eq!(
        chain.get_balance(bob.get_address(), TokenId::Ziesha)?,
        Amount(0)
    );

    rollback_till_empty(&mut chain)?;

    Ok(())
}

#[test]
fn test_balances_are_correct_after_tx() -> Result<(), BlockchainError> {
    let miner = TxBuilder::new(&Vec::from("MINER"));
    let alice = TxBuilder::new(&Vec::from("ABC"));
    let bob = TxBuilder::new(&Vec::from("CBA"));

    let mut chain = KvStoreChain::new(db::RamKvStore::new(), easy_config())?;

    // Alice: 10000 Bob: 0
    assert_eq!(
        chain.get_balance(alice.get_address(), TokenId::Ziesha)?,
        Amount(10000)
    );
    assert_eq!(
        chain.get_balance(bob.get_address(), TokenId::Ziesha)?,
        Amount(0)
    );

    // Alice -> 2700 -> Bob (Fee 300)
    chain.apply_block(
        &chain
            .draft_block(
                1,
                &[alice.create_transaction(
                    "".into(),
                    bob.get_address(),
                    Money::ziesha(2700),
                    Money::ziesha(300),
                    1,
                )],
                &miner,
                true,
            )?
            .unwrap()
            .block,
        true,
    )?;
    assert_eq!(
        chain.get_balance(alice.get_address(), TokenId::Ziesha)?,
        Amount(7000)
    );
    assert_eq!(
        chain.get_balance(bob.get_address(), TokenId::Ziesha)?,
        Amount(2700)
    );

    // Bob -> 2600 -> Alice (Fee 200) (BALANCE INSUFFICIENT!)
    chain.apply_block(
        &chain
            .draft_block(
                1,
                &[bob.create_transaction(
                    "".into(),
                    alice.get_address(),
                    Money::ziesha(2600),
                    Money::ziesha(200),
                    1,
                )],
                &miner,
                true,
            )?
            .unwrap()
            .block,
        true,
    )?;
    assert_eq!(
        chain.get_balance(alice.get_address(), TokenId::Ziesha)?,
        Amount(7000)
    );
    assert_eq!(
        chain.get_balance(bob.get_address(), TokenId::Ziesha)?,
        Amount(2700)
    );

    // Bob -> 2600 -> Alice (Fee 200)
    chain.apply_block(
        &chain
            .draft_block(
                2,
                &[bob.create_transaction(
                    "".into(),
                    alice.get_address(),
                    Money::ziesha(2600),
                    Money::ziesha(100),
                    1,
                )],
                &miner,
                true,
            )?
            .unwrap()
            .block,
        true,
    )?;
    assert_eq!(
        chain.get_balance(alice.get_address(), TokenId::Ziesha)?,
        Amount(9600)
    );
    assert_eq!(
        chain.get_balance(bob.get_address(), TokenId::Ziesha)?,
        Amount(0)
    );

    // Alice -> 100 -> Alice (Fee 200) (SELF PAYMENT NOT ALLOWED)
    chain.apply_block(
        &chain
            .draft_block(
                3,
                &[alice.create_transaction(
                    "".into(),
                    alice.get_address(),
                    Money::ziesha(100),
                    Money::ziesha(200),
                    2,
                )],
                &miner,
                true,
            )?
            .unwrap()
            .block,
        true,
    )?;
    assert_eq!(
        chain.get_balance(alice.get_address(), TokenId::Ziesha)?,
        Amount(9400)
    );
    assert_eq!(
        chain.get_balance(bob.get_address(), TokenId::Ziesha)?,
        Amount(0)
    );

    // Alice -> 20000 -> Alice (Fee 9400) (SELF PAYMENT NOT ALLOWED)
    chain.apply_block(
        &chain
            .draft_block(
                4,
                &[alice.create_transaction(
                    "".into(),
                    alice.get_address(),
                    Money::ziesha(20000),
                    Money::ziesha(9400),
                    3,
                )],
                &miner,
                true,
            )?
            .unwrap()
            .block,
        true,
    )?;
    assert_eq!(
        chain.get_balance(alice.get_address(), TokenId::Ziesha)?,
        Amount(0)
    );
    assert_eq!(
        chain.get_balance(bob.get_address(), TokenId::Ziesha)?,
        Amount(0)
    );

    rollback_till_empty(&mut chain)?;

    Ok(())
}

#[test]
fn test_genesis_is_not_replaceable() -> Result<(), BlockchainError> {
    let conf = blockchain::get_blockchain_config();
    let mut chain = KvStoreChain::new(db::RamKvStore::new(), conf.clone())?;
    assert_eq!(1, chain.get_height()?);

    let first_block = chain.get_block(0)?;
    assert_eq!(conf.genesis.block.header.hash(), first_block.header.hash());

    let mut another_conf = conf.clone();
    another_conf.genesis.block.header.proof_of_work.timestamp += 1;

    assert!(matches!(
        chain.extend(0, &[another_conf.genesis.block]),
        Err(BlockchainError::ExtendFromGenesis)
    ));

    rollback_till_empty(&mut chain)?;

    Ok(())
}

#[test]
fn test_chain_should_apply_mined_draft_block() -> Result<(), BlockchainError> {
    let wallet_miner = TxBuilder::new(&Vec::from("MINER"));
    let wallet1 = TxBuilder::new(&Vec::from("ABC"));
    let wallet2 = TxBuilder::new(&Vec::from("CBA"));

    let mut conf = blockchain::get_test_blockchain_config();
    conf.minimum_pow_difficulty = Difficulty(0x0000ffff);
    conf.genesis.block.header.proof_of_work.target = conf.minimum_pow_difficulty;
    conf.genesis.block.body.push(Transaction {
        memo: "".into(),
        src: None,
        data: TransactionData::RegularSend {
            entries: vec![RegularSendEntry {
                dst: wallet1.get_address(),
                amount: Money::ziesha(10_000_000),
            }],
        },
        nonce: 4,
        fee: Money::ziesha(0),
        sig: Signature::Unsigned,
    });

    let mut chain = KvStoreChain::new(db::RamKvStore::new(), conf)?;

    let t1 = wallet1.create_transaction(
        "".into(),
        wallet2.get_address(),
        Money::ziesha(100),
        Money::ziesha(0),
        1,
    );
    let mempool = vec![t1];
    let mut draft = chain
        .draft_block(1650000000, &mempool, &wallet_miner, true)?
        .unwrap();

    assert!(matches!(
        chain.apply_block(&draft.block, true),
        Err(BlockchainError::DifficultyTargetUnmet)
    ));

    mine_block(&chain, &mut draft)?;
    chain.apply_block(&draft.block, true)?;

    let height = chain.get_height()?;
    assert_eq!(2, height);

    let last_block = chain.get_block(height - 1)?;
    let w2_address = wallet2.get_address();
    assert_eq!(
        last_block.body[1].data.clone(),
        TransactionData::RegularSend {
            entries: vec![RegularSendEntry {
                dst: w2_address,
                amount: Money::ziesha(100)
            }]
        }
    );

    let balance = chain.get_balance(wallet2.get_address(), TokenId::Ziesha)?;
    let account = chain.get_account(wallet2.get_address())?;
    assert_eq!(Amount(100), balance);
    assert_eq!(0, account.nonce);

    rollback_till_empty(&mut chain)?;

    Ok(())
}

#[test]
fn test_chain_should_not_draft_invalid_transactions() -> Result<(), BlockchainError> {
    let wallet_miner = TxBuilder::new(&Vec::from("MINER"));
    let wallet1 = TxBuilder::new(&Vec::from("ABC"));
    let wallet2 = TxBuilder::new(&Vec::from("CBA"));

    let mut conf = blockchain::get_test_blockchain_config();
    conf.genesis.block.body.push(Transaction {
        memo: "".into(),
        src: None,
        data: TransactionData::RegularSend {
            entries: vec![RegularSendEntry {
                dst: wallet1.get_address(),
                amount: Money::ziesha(10_000_000),
            }],
        },
        nonce: 4,
        fee: Money::ziesha(0),
        sig: Signature::Unsigned,
    });

    let mut chain = KvStoreChain::new(db::RamKvStore::new(), conf)?;

    let t_valid = wallet1.create_transaction(
        "".into(),
        wallet2.get_address(),
        Money::ziesha(200),
        Money::ziesha(0),
        1,
    );
    let t_invalid_unsigned = TransactionAndDelta {
        tx: Transaction {
            memo: "".into(),
            src: Some(wallet1.get_address()),
            data: TransactionData::RegularSend {
                entries: vec![RegularSendEntry {
                    dst: wallet2.get_address(),
                    amount: Money::ziesha(300),
                }],
            },
            nonce: 1,
            fee: Money::ziesha(0),
            sig: Signature::Unsigned, // invalid transaction
        },
        state_delta: None,
    };
    let t_invalid_from_treasury = TransactionAndDelta {
        tx: Transaction {
            memo: "".into(),
            src: None,
            data: TransactionData::RegularSend {
                entries: vec![RegularSendEntry {
                    dst: wallet2.get_address(),
                    amount: Money::ziesha(500),
                }],
            },
            nonce: 1,
            fee: Money::ziesha(0),
            sig: Signature::Unsigned, // invalid transaction
        },
        state_delta: None,
    };
    let mempool = vec![t_valid, t_invalid_unsigned, t_invalid_from_treasury];
    let mut draft = chain
        .draft_block(1650000000, &mempool, &wallet_miner, true)?
        .unwrap();

    mine_block(&chain, &mut draft)?;

    assert_eq!(2, draft.block.body.len());
    assert_eq!(Some(wallet1.get_address()), draft.block.body[1].src);

    rollback_till_empty(&mut chain)?;

    Ok(())
}

#[test]
fn test_chain_should_draft_all_valid_transactions() -> Result<(), BlockchainError> {
    let wallet_miner = TxBuilder::new(&Vec::from("MINER"));
    let wallet1 = TxBuilder::new(&Vec::from("ABCD"));
    let wallet2 = TxBuilder::new(&Vec::from("CBAD"));

    let mut conf = blockchain::get_test_blockchain_config();
    conf.genesis.block.body.push(Transaction {
        memo: "".into(),
        src: None,
        data: TransactionData::RegularSend {
            entries: vec![RegularSendEntry {
                dst: wallet1.get_address(),
                amount: Money::ziesha(10_000_000),
            }],
        },
        nonce: 4,
        fee: Money::ziesha(0),
        sig: Signature::Unsigned,
    });

    let mut chain = KvStoreChain::new(db::RamKvStore::new(), conf)?;

    let t1 = wallet1.create_transaction(
        "".into(),
        wallet2.get_address(),
        Money::ziesha(3000),
        Money::ziesha(0),
        1,
    );
    let t2 = wallet1.create_transaction(
        "".into(),
        wallet2.get_address(),
        Money::ziesha(4000),
        Money::ziesha(0),
        2,
    );

    let mempool = vec![t1, t2];
    let mut draft = chain
        .draft_block(1650000000, &mempool, &wallet_miner, true)?
        .unwrap();

    mine_block(&chain, &mut draft)?;

    chain.apply_block(&draft.block, true)?;

    assert_eq!(3, draft.block.body.len());

    let account1 = chain.get_balance(wallet1.get_address(), TokenId::Ziesha)?;
    let account2 = chain.get_balance(wallet2.get_address(), TokenId::Ziesha)?;
    assert_eq!(Amount(10_000_000 - 7000), account1);
    assert_eq!(Amount(7000), account2);

    rollback_till_empty(&mut chain)?;

    Ok(())
}

#[test]
fn test_chain_should_rollback_applied_block() -> Result<(), BlockchainError> {
    let wallet_miner = TxBuilder::new(&Vec::from("MINER"));
    let wallet1 = TxBuilder::new(&Vec::from("ABC"));
    let wallet2 = TxBuilder::new(&Vec::from("CBA"));

    let mut conf = blockchain::get_test_blockchain_config();
    conf.genesis.block.body.push(Transaction {
        memo: "".into(),
        src: None,
        data: TransactionData::RegularSend {
            entries: vec![RegularSendEntry {
                dst: wallet1.get_address(),
                amount: Money::ziesha(10_000_000),
            }],
        },
        nonce: 4,
        fee: Money::ziesha(0),
        sig: Signature::Unsigned,
    });

    let mut chain = KvStoreChain::new(db::RamKvStore::new(), conf)?;

    let t1 = wallet1.create_transaction(
        "".into(),
        wallet2.get_address(),
        Money::ziesha(1_000_000),
        Money::ziesha(0),
        1,
    );
    let mut mempool = vec![t1];
    let mut draft = chain
        .draft_block(1650000000, &mempool, &wallet_miner, true)?
        .unwrap();

    mine_block(&chain, &mut draft)?;

    chain.apply_block(&draft.block, true)?;

    let t2 = wallet1.create_transaction(
        "".into(),
        wallet2.get_address(),
        Money::ziesha(500_000),
        Money::ziesha(0),
        2,
    );
    mempool.push(t2);

    let mut draft = chain
        .draft_block(1650000001, &mempool, &wallet_miner, true)?
        .unwrap();

    mine_block(&chain, &mut draft)?;

    let prev_checksum = chain.database.checksum::<Hasher>()?;

    chain.apply_block(&draft.block, true)?;

    let height = chain.get_height()?;
    assert_eq!(3, height);

    let last_block = chain.get_block(height - 1)?;
    assert_eq!(
        last_block.body[1].data.clone(),
        TransactionData::RegularSend {
            entries: vec![RegularSendEntry {
                dst: wallet2.get_address(),
                amount: Money::ziesha(500_000)
            }]
        }
    );

    let after_checksum = chain.database.checksum::<Hasher>()?;

    chain.rollback()?;

    let rollbacked_checksum = chain.database.checksum::<Hasher>()?;

    assert_ne!(prev_checksum, after_checksum);
    assert_eq!(prev_checksum, rollbacked_checksum);

    let height = chain.get_height()?;
    assert_eq!(2, height);

    let account = chain.get_account(wallet2.get_address())?;
    let balance = chain.get_balance(wallet2.get_address(), TokenId::Ziesha)?;
    assert_eq!(Amount(1_000_000), balance);
    assert_eq!(0, account.nonce);

    rollback_till_empty(&mut chain)?;

    Ok(())
}

fn mine_block<B: Blockchain>(chain: &B, draft: &mut BlockAndPatch) -> Result<(), BlockchainError> {
    let pow_key = chain.pow_key(draft.block.header.number)?;

    if draft.block.header.meets_target(pow_key.as_slice()) {
        return Ok(());
    }

    draft.block.header.proof_of_work.nonce = 0;
    while !draft.block.header.meets_target(pow_key.as_slice()) {
        draft.block.header.proof_of_work.nonce += 1;
    }

    Ok(())
}
