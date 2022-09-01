// Copyright (c) Aptos
// SPDX-License-Identifier: Apache-2.0

use crate::{
    core_mempool::{CoreMempool, MempoolTransaction, TimelineState, TtlCache},
    tests::common::{
        add_signed_txn, add_txn, add_txns_to_mempool, exist_in_metrics_cache, setup_mempool,
        TestTransaction,
    },
};
use aptos_config::config::NodeConfig;
use aptos_crypto::HashValue;
use aptos_types::mempool_status::MempoolStatusCode;
use aptos_types::{account_config::AccountSequenceInfo, transaction::SignedTransaction};
use std::{
    collections::HashSet,
    time::{Duration, SystemTime},
};

#[test]
fn test_transaction_ordering_only_seqnos() {
    let (mut mempool, mut consensus) = setup_mempool();

    // Default ordering: gas price
    let mut transactions = add_txns_to_mempool(
        &mut mempool,
        vec![TestTransaction::new(0, 0, 3), TestTransaction::new(1, 0, 5)],
    );
    assert_eq!(
        consensus.get_block(&mut mempool, 1, 1024),
        vec!(transactions[1].clone())
    );
    assert_eq!(
        consensus.get_block(&mut mempool, 1, 1024),
        vec!(transactions[0].clone())
    );

    // Second level ordering: expiration time
    let (mut mempool, mut consensus) = setup_mempool();
    transactions = add_txns_to_mempool(
        &mut mempool,
        vec![TestTransaction::new(0, 0, 1), TestTransaction::new(1, 0, 1)],
    );
    for transaction in &transactions {
        assert_eq!(
            consensus.get_block(&mut mempool, 1, 1024),
            vec![transaction.clone()]
        );
    }

    // Last level: for same account it should be by sequence number
    let (mut mempool, mut consensus) = setup_mempool();
    transactions = add_txns_to_mempool(
        &mut mempool,
        vec![
            TestTransaction::new(1, 0, 7),
            TestTransaction::new(1, 1, 5),
            TestTransaction::new(1, 2, 1),
            TestTransaction::new(1, 3, 6),
        ],
    );
    for transaction in &transactions {
        assert_eq!(
            consensus.get_block(&mut mempool, 1, 1024),
            vec![transaction.clone()]
        );
    }
}

#[test]
fn test_transaction_ordering_only_crsns() {
    let (mut mempool, mut consensus) = setup_mempool();

    // Default ordering: gas price
    let mut transactions = add_txns_to_mempool(
        &mut mempool,
        vec![
            TestTransaction::new(0, 0, 3).crsn(0),
            TestTransaction::new(1, 0, 5).crsn(0),
        ],
    );
    assert_eq!(
        consensus.get_block(&mut mempool, 1, 1024),
        vec!(transactions[1].clone())
    );
    assert_eq!(
        consensus.get_block(&mut mempool, 1, 1024),
        vec!(transactions[0].clone())
    );

    // Second level ordering: expiration time
    let (mut mempool, mut consensus) = setup_mempool();
    transactions = add_txns_to_mempool(
        &mut mempool,
        vec![
            TestTransaction::new(0, 0, 1).crsn(0),
            TestTransaction::new(1, 0, 1).crsn(0),
        ],
    );
    for transaction in &transactions {
        assert_eq!(
            consensus.get_block(&mut mempool, 1, 1024),
            vec![transaction.clone()]
        );
    }

    // Last level: for same account it should be highest gas price
    // first with ties broken for lowest sequence nonce
    let (mut mempool, mut consensus) = setup_mempool();
    transactions = add_txns_to_mempool(
        &mut mempool,
        vec![
            TestTransaction::new(1, 0, 7).crsn(0),
            TestTransaction::new(1, 4, 6).crsn(0),
            TestTransaction::new(1, 1, 5).crsn(0),
            TestTransaction::new(1, 2, 5).crsn(0),
            TestTransaction::new(1, 3, 1).crsn(0),
        ],
    );
    for transaction in &transactions {
        assert_eq!(
            consensus.get_block(&mut mempool, 1, 1024),
            vec![transaction.clone()]
        );
    }
}

#[test]
fn test_transaction_nonblocking_crsns() {
    let (mut mempool, mut consensus) = setup_mempool();

    // no transaction with sequence number 1 sent, but the transactions won't block on it.
    let transactions = add_txns_to_mempool(
        &mut mempool,
        vec![
            TestTransaction::new(1, 0, 7).crsn(0),
            TestTransaction::new(1, 4, 6).crsn(0),
            TestTransaction::new(1, 2, 5).crsn(0),
            TestTransaction::new(1, 3, 1).crsn(0),
        ],
    );
    for transaction in &transactions {
        assert_eq!(
            consensus.get_block(&mut mempool, 1, 1024),
            vec![transaction.clone()]
        );
    }
}

#[test]
fn test_transaction_eviction_crsns() {
    let (mut mempool, mut consensus) = setup_mempool();

    let to_be_removed_txns = add_txns_to_mempool(
        &mut mempool,
        vec![
            TestTransaction::new(1, 2, 5).crsn(0),
            TestTransaction::new(1, 0, 7).crsn(0),
        ],
    );
    let transactions = add_txns_to_mempool(
        &mut mempool,
        vec![
            TestTransaction::new(1, 4, 6).crsn(3),
            TestTransaction::new(1, 3, 1).crsn(3),
        ],
    );

    for transaction in &transactions {
        assert_eq!(
            consensus.get_block(&mut mempool, 1, 1024),
            vec![transaction.clone()]
        );
    }

    // These transactions should have been evicted because the account's min_nonce got bumped
    for _transaction in &to_be_removed_txns {
        assert_eq!(consensus.get_block(&mut mempool, 1, 1024), vec![]);
    }
}

#[test]
fn test_metric_cache_add_local_txns() {
    let (mut mempool, _) = setup_mempool();
    let txns = add_txns_to_mempool(
        &mut mempool,
        vec![TestTransaction::new(0, 0, 1), TestTransaction::new(1, 0, 2)],
    );
    // Check txns' timestamps exist in metrics_cache.
    assert_eq!(exist_in_metrics_cache(&mempool, &txns[0]), true);
    assert_eq!(exist_in_metrics_cache(&mempool, &txns[1]), true);
}

#[test]
fn test_update_transaction_in_mempool() {
    let (mut mempool, mut consensus) = setup_mempool();
    let txns = add_txns_to_mempool(
        &mut mempool,
        vec![TestTransaction::new(0, 0, 1), TestTransaction::new(1, 0, 2)],
    );
    let fixed_txns = add_txns_to_mempool(&mut mempool, vec![TestTransaction::new(0, 0, 5)]);

    // Check that first transactions pops up first
    assert_eq!(
        consensus.get_block(&mut mempool, 1, 1024),
        vec![fixed_txns[0].clone()]
    );
    assert_eq!(
        consensus.get_block(&mut mempool, 1, 1024),
        vec![txns[1].clone()]
    );
}

#[test]
fn test_update_transaction_in_mempool_crsn() {
    let (mut mempool, mut consensus) = setup_mempool();
    let txns = add_txns_to_mempool(
        &mut mempool,
        vec![
            TestTransaction::new(0, 0, 1).crsn(0),
            TestTransaction::new(1, 0, 2).crsn(0),
        ],
    );
    let fixed_txns = add_txns_to_mempool(&mut mempool, vec![TestTransaction::new(0, 0, 5).crsn(0)]);

    // Check that first transactions pops up first
    assert_eq!(
        consensus.get_block(&mut mempool, 1, 1024),
        vec![fixed_txns[0].clone()]
    );
    assert_eq!(
        consensus.get_block(&mut mempool, 1, 1024),
        vec![txns[1].clone()]
    );
}

#[test]
fn test_ignore_same_transaction_submitted_to_mempool() {
    let (mut mempool, _) = setup_mempool();
    let _ = add_txns_to_mempool(&mut mempool, vec![TestTransaction::new(0, 0, 0)]);
    let ret = add_txn(&mut mempool, TestTransaction::new(0, 0, 0));
    assert!(ret.is_ok())
}

#[test]
fn test_ignore_same_transaction_submitted_to_mempool_crsn() {
    let (mut mempool, _) = setup_mempool();
    let _ = add_txns_to_mempool(&mut mempool, vec![TestTransaction::new(0, 0, 0).crsn(0)]);
    let ret = add_txn(&mut mempool, TestTransaction::new(0, 0, 0).crsn(0));
    assert!(ret.is_ok())
}

#[test]
fn test_fail_for_same_gas_amount_and_not_same_expiration_time() {
    let (mut mempool, _) = setup_mempool();
    let _ = add_txns_to_mempool(&mut mempool, vec![TestTransaction::new(0, 0, 0)]);
    let txn = TestTransaction::new(0, 0, 0)
        .make_signed_transaction_with_expiration_time(u64::max_value() - 1000);
    let ret = add_signed_txn(&mut mempool, txn);
    assert!(ret.is_err())
}

#[test]
fn test_fail_for_same_gas_amount_and_not_same_expiration_time_crsn() {
    let (mut mempool, _) = setup_mempool();
    let _ = add_txns_to_mempool(&mut mempool, vec![TestTransaction::new(0, 0, 0).crsn(0)]);
    let txn = TestTransaction::new(0, 0, 0)
        .crsn(0)
        .make_signed_transaction_with_expiration_time(u64::max_value() - 1000);
    let ret = add_signed_txn(&mut mempool, txn);
    assert!(ret.is_err())
}

#[test]
fn test_update_invalid_transaction_in_mempool() {
    let (mut mempool, mut consensus) = setup_mempool();
    let txns = add_txns_to_mempool(
        &mut mempool,
        vec![TestTransaction::new(0, 0, 1), TestTransaction::new(1, 0, 2)],
    );
    let updated_txn = TestTransaction::make_signed_transaction_with_max_gas_amount(
        &TestTransaction::new(0, 0, 5),
        200,
    );
    let _added_tnx = add_signed_txn(&mut mempool, updated_txn);

    // Since both gas price and mas gas amount were updated, the ordering should not have changed.
    // The second transaction with gas price 2 should come first.
    assert_eq!(
        consensus.get_block(&mut mempool, 1, 1024),
        vec![txns[1].clone()]
    );
    let next_tnx = consensus.get_block(&mut mempool, 1, 1024);
    assert_eq!(next_tnx, vec![txns[0].clone()]);
    assert_eq!(next_tnx[0].gas_unit_price(), 1);
}

#[test]
fn test_update_invalid_transaction_in_mempool_crsn() {
    let (mut mempool, mut consensus) = setup_mempool();
    let txns = add_txns_to_mempool(
        &mut mempool,
        vec![
            TestTransaction::new(0, 0, 1).crsn(0),
            TestTransaction::new(1, 0, 2).crsn(0),
        ],
    );
    let updated_txn = TestTransaction::make_signed_transaction_with_max_gas_amount(
        &TestTransaction::new(0, 0, 5).crsn(0),
        200,
    );
    let _added_txn = add_signed_txn(&mut mempool, updated_txn);

    // Since both gas price and mas gas amount were updated, the ordering should not have changed.
    // The second transaction with gas price 2 should come first.
    assert_eq!(
        consensus.get_block(&mut mempool, 1, 1024),
        vec![txns[1].clone()]
    );
    let next_txn = consensus.get_block(&mut mempool, 1, 1024);
    assert_eq!(next_txn, vec![txns[0].clone()]);
    assert_eq!(next_txn[0].gas_unit_price(), 1);
}

#[test]
fn test_remove_transaction() {
    let (mut pool, mut consensus) = setup_mempool();

    // Test normal flow.
    let txns = add_txns_to_mempool(
        &mut pool,
        vec![TestTransaction::new(0, 0, 1), TestTransaction::new(0, 1, 2)],
    );
    for txn in txns {
        pool.remove_transaction(&txn.sender(), txn.sequence_number(), false);
    }
    let new_txns = add_txns_to_mempool(
        &mut pool,
        vec![TestTransaction::new(1, 0, 3), TestTransaction::new(1, 1, 4)],
    );
    // Should return only txns from new_txns.
    assert_eq!(
        consensus.get_block(&mut pool, 1, 1024),
        vec!(new_txns[0].clone())
    );
    assert_eq!(
        consensus.get_block(&mut pool, 1, 1024),
        vec!(new_txns[1].clone())
    );
}

#[test]
fn test_remove_transaction_crsn() {
    let (mut pool, mut consensus) = setup_mempool();

    // Test normal flow.
    let txns = add_txns_to_mempool(
        &mut pool,
        vec![
            TestTransaction::new(0, 0, 1).crsn(0),
            TestTransaction::new(0, 1, 2).crsn(0),
        ],
    );
    for txn in txns {
        pool.remove_transaction(&txn.sender(), txn.sequence_number(), false);
    }
    let new_txns = add_txns_to_mempool(
        &mut pool,
        vec![
            TestTransaction::new(1, 1, 4).crsn(0),
            TestTransaction::new(1, 0, 3).crsn(0),
        ],
    );
    // Should return only txns from new_txns.
    assert_eq!(
        consensus.get_block(&mut pool, 1, 1024),
        vec!(new_txns[0].clone())
    );
    assert_eq!(
        consensus.get_block(&mut pool, 1, 1024),
        vec!(new_txns[1].clone())
    );
}

#[test]
fn test_system_ttl() {
    // Created mempool with system_transaction_timeout = 0.
    // All transactions are supposed to be evicted on next gc run.
    let mut config = NodeConfig::random();
    config.mempool.system_transaction_timeout_secs = 0;
    let mut mempool = CoreMempool::new(&config);

    add_txn(&mut mempool, TestTransaction::new(0, 0, 10)).unwrap();

    // Reset system ttl timeout.
    mempool.system_transaction_timeout = Duration::from_secs(10);
    // Add new transaction. Should be valid for 10 seconds.
    let transaction = TestTransaction::new(1, 0, 1);
    add_txn(&mut mempool, transaction.clone()).unwrap();

    // GC routine should clear transaction from first insert but keep last one.
    mempool.gc();
    let batch = mempool.get_batch(1, 1024, HashSet::new());
    assert_eq!(vec![transaction.make_signed_transaction()], batch);
}

#[test]
fn test_commit_callback() {
    // Consensus commit callback should unlock txns in parking lot.
    let mut pool = setup_mempool().0;
    // Insert transaction with sequence number 6 to pool (while last known executed transaction is 0).
    let txns = add_txns_to_mempool(&mut pool, vec![TestTransaction::new(1, 6, 1)]);

    // Check that pool is empty.
    assert!(pool.get_batch(1, 1024, HashSet::new()).is_empty());
    // Transaction 5 got back from consensus.
    pool.remove_transaction(&TestTransaction::get_address(1), 5, false);
    // Verify that we can execute transaction 6.
    assert_eq!(pool.get_batch(1, 1024, HashSet::new())[0], txns[0]);
}

#[test]
fn test_sequence_number_cache() {
    // Checks potential race where StateDB is lagging.
    let mut pool = setup_mempool().0;
    // Callback from consensus should set current sequence number for account.
    pool.remove_transaction(&TestTransaction::get_address(1), 5, false);

    // Try to add transaction with sequence number 6 to pool (while last known executed transaction
    // for AC is 0).
    add_txns_to_mempool(&mut pool, vec![TestTransaction::new(1, 6, 1)]);
    // Verify that we can execute transaction 6.
    assert_eq!(pool.get_batch(1, 1024, HashSet::new()).len(), 1);
}

#[test]
fn test_reset_sequence_number_on_failure() {
    let mut pool = setup_mempool().0;
    // Add two transactions for account.
    add_txns_to_mempool(
        &mut pool,
        vec![TestTransaction::new(1, 0, 1), TestTransaction::new(1, 1, 1)],
    );

    // Notify mempool about failure in arbitrary order
    pool.remove_transaction(&TestTransaction::get_address(1), 0, true);
    pool.remove_transaction(&TestTransaction::get_address(1), 1, true);

    // Verify that new transaction for this account can be added.
    assert!(add_txn(&mut pool, TestTransaction::new(1, 0, 1)).is_ok());
}

#[test]
fn test_timeline() {
    let mut pool = setup_mempool().0;
    add_txns_to_mempool(
        &mut pool,
        vec![
            TestTransaction::new(1, 0, 1),
            TestTransaction::new(1, 1, 1),
            TestTransaction::new(1, 3, 1),
            TestTransaction::new(1, 5, 1),
        ],
    );
    let view = |txns: Vec<SignedTransaction>| -> Vec<u64> {
        txns.iter()
            .map(SignedTransaction::sequence_number)
            .collect()
    };
    let (timeline, _) = pool.read_timeline(0, 10);
    assert_eq!(view(timeline), vec![0, 1]);
    // Txns 3 and 5 should be in parking lot.
    assert_eq!(2, pool.get_parking_lot_size());

    // Add txn 2 to unblock txn3.
    add_txns_to_mempool(&mut pool, vec![TestTransaction::new(1, 2, 1)]);
    let (timeline, _) = pool.read_timeline(0, 10);
    assert_eq!(view(timeline), vec![0, 1, 2, 3]);
    // Txn 5 should be in parking lot.
    assert_eq!(1, pool.get_parking_lot_size());

    // Try different start read position.
    let (timeline, _) = pool.read_timeline(2, 10);
    assert_eq!(view(timeline), vec![2, 3]);

    // Simulate callback from consensus to unblock txn 5.
    pool.remove_transaction(&TestTransaction::get_address(1), 4, false);
    let (timeline, _) = pool.read_timeline(0, 10);
    assert_eq!(view(timeline), vec![5]);
    // check parking lot is empty
    assert_eq!(0, pool.get_parking_lot_size());
}

#[test]
fn test_capacity() {
    let mut config = NodeConfig::random();
    config.mempool.capacity = 1;
    config.mempool.system_transaction_timeout_secs = 0;
    let mut pool = CoreMempool::new(&config);

    // Error on exceeding limit.
    add_txn(&mut pool, TestTransaction::new(1, 0, 1)).unwrap();
    assert!(add_txn(&mut pool, TestTransaction::new(1, 1, 1)).is_err());

    // Commit transaction and free space.
    pool.remove_transaction(&TestTransaction::get_address(1), 0, false);
    assert!(add_txn(&mut pool, TestTransaction::new(1, 1, 1)).is_ok());

    // Fill it up and check that GC routine will clear space.
    assert!(add_txn(&mut pool, TestTransaction::new(1, 2, 1)).is_err());
    pool.gc();
    assert!(add_txn(&mut pool, TestTransaction::new(1, 2, 1)).is_ok());
}

#[test]
fn test_capacity_bytes() {
    let capacity_bytes = 2_048;

    // Get transactions to add.
    let address = 1;
    let mut size_bytes: usize = 0;
    let mut seq_no = 1_000;
    let mut txns = vec![];
    let last_txn;
    loop {
        let txn = new_test_mempool_transaction(address, seq_no);
        let txn_bytes = txn.get_estimated_bytes();

        if size_bytes <= capacity_bytes {
            txns.push(txn);
            seq_no -= 1;
            size_bytes += txn_bytes;
        } else {
            last_txn = Some(txn);
            break;
        }
    }
    assert!(!txns.is_empty());
    assert!(last_txn.is_some());

    // Set exact limit
    let capacity_bytes = size_bytes;

    let mut config = NodeConfig::random();
    config.mempool.capacity = 1_000; // Won't hit this limit.
    config.mempool.capacity_bytes = capacity_bytes;
    config.mempool.system_transaction_timeout_secs = 0;
    let mut pool = CoreMempool::new(&config);

    for _i in 0..2 {
        txns.clone().into_iter().for_each(|txn| {
            let status = pool.add_txn(
                txn.txn,
                txn.ranking_score,
                txn.sequence_info.account_sequence_number_type,
                txn.timeline_state,
            );
            assert_eq!(status.code, MempoolStatusCode::Accepted);
        });

        if let Some(txn) = last_txn.clone() {
            let status = pool.add_txn(
                txn.txn,
                txn.ranking_score,
                txn.sequence_info.account_sequence_number_type,
                txn.timeline_state,
            );
            assert_eq!(status.code, MempoolStatusCode::MempoolIsFull);
        }
        // Check that GC returns size to zero.
        pool.gc();
    }
}

fn new_test_mempool_transaction(address: usize, sequence_number: u64) -> MempoolTransaction {
    let signed_txn = TestTransaction::new(address, sequence_number, 1).make_signed_transaction();
    MempoolTransaction::new(
        signed_txn,
        Duration::from_secs(1),
        1,
        TimelineState::NotReady,
        AccountSequenceInfo::Sequential(0),
    )
}

#[test]
fn test_parking_lot_eviction() {
    let mut config = NodeConfig::random();
    config.mempool.capacity = 5;
    let mut pool = CoreMempool::new(&config);
    // Add transactions with the following sequence numbers to Mempool.
    for seq in &[0, 1, 2, 9, 10] {
        add_txn(&mut pool, TestTransaction::new(1, *seq, 1)).unwrap();
    }
    // Mempool is full. Insert few txns for other account.
    for seq in &[0, 1] {
        add_txn(&mut pool, TestTransaction::new(0, *seq, 1)).unwrap();
    }
    // Make sure that we have correct txns in Mempool.
    let mut txns: Vec<_> = pool
        .get_batch(5, 5120, HashSet::new())
        .iter()
        .map(SignedTransaction::sequence_number)
        .collect();
    txns.sort_unstable();
    assert_eq!(txns, vec![0, 0, 1, 1, 2]);

    // Make sure we can't insert any new transactions, cause parking lot supposed to be empty by now.
    assert!(add_txn(&mut pool, TestTransaction::new(0, 2, 1)).is_err());
}

#[test]
fn test_parking_lot_evict_only_for_ready_txn_insertion() {
    let mut config = NodeConfig::random();
    config.mempool.capacity = 6;
    let mut pool = CoreMempool::new(&config);
    // Add transactions with the following sequence numbers to Mempool.
    for seq in &[0, 1, 2, 9, 10, 11] {
        add_txn(&mut pool, TestTransaction::new(1, *seq, 1)).unwrap();
    }

    // Try inserting for ready txs.
    let ready_seq_nums = vec![3, 4];
    for seq in ready_seq_nums {
        add_txn(&mut pool, TestTransaction::new(1, seq, 1)).unwrap();
    }

    // Make sure that we have correct txns in Mempool.
    let mut txns: Vec<_> = pool
        .get_batch(5, 5120, HashSet::new())
        .iter()
        .map(SignedTransaction::sequence_number)
        .collect();
    txns.sort_unstable();
    assert_eq!(txns, vec![0, 1, 2, 3, 4]);

    // Trying to insert a tx that would not be ready after inserting should fail.
    let not_ready_seq_nums = vec![6, 8, 12, 14];
    for seq in not_ready_seq_nums {
        assert!(add_txn(&mut pool, TestTransaction::new(1, seq, 1)).is_err());
    }
}

#[test]
fn test_gc_ready_transaction() {
    let mut pool = setup_mempool().0;
    add_txn(&mut pool, TestTransaction::new(1, 0, 1)).unwrap();

    // Insert in the middle transaction that's going to be expired.
    let txn = TestTransaction::new(1, 1, 1).make_signed_transaction_with_expiration_time(0);
    pool.add_txn(
        txn,
        1,
        AccountSequenceInfo::Sequential(0),
        TimelineState::NotReady,
    );

    // Insert few transactions after it.
    // They are supposed to be ready because there's a sequential path from 0 to them.
    add_txn(&mut pool, TestTransaction::new(1, 2, 1)).unwrap();
    add_txn(&mut pool, TestTransaction::new(1, 3, 1)).unwrap();

    // Check that all txns are ready.
    let (timeline, _) = pool.read_timeline(0, 10);
    assert_eq!(timeline.len(), 4);

    // GC expired transaction.
    pool.gc_by_expiration_time(Duration::from_secs(1));

    // Make sure txns 2 and 3 became not ready and we can't read them from any API.
    let block = pool.get_batch(1, 1024, HashSet::new());
    assert_eq!(block.len(), 1);
    assert_eq!(block[0].sequence_number(), 0);

    let (timeline, _) = pool.read_timeline(0, 10);
    assert_eq!(timeline.len(), 1);
    assert_eq!(timeline[0].sequence_number(), 0);
}

#[test]
fn test_clean_stuck_transactions() {
    let mut pool = setup_mempool().0;
    for seq in 0..5 {
        add_txn(&mut pool, TestTransaction::new(0, seq, 1)).unwrap();
    }
    let db_sequence_number = 10;
    let txn = TestTransaction::new(0, db_sequence_number, 1).make_signed_transaction();
    pool.add_txn(
        txn,
        1,
        AccountSequenceInfo::Sequential(db_sequence_number),
        TimelineState::NotReady,
    );
    let block = pool.get_batch(1, 1024, HashSet::new());
    assert_eq!(block.len(), 1);
    assert_eq!(block[0].sequence_number(), 10);
}

#[test]
fn test_ttl_cache() {
    let mut cache = TtlCache::new(2, Duration::from_secs(1));
    // Test basic insertion.
    cache.insert(1, 1);
    cache.insert(1, 2);
    cache.insert(2, 2);
    cache.insert(1, 3);
    assert_eq!(cache.get(&1), Some(&3));
    assert_eq!(cache.get(&2), Some(&2));
    assert_eq!(cache.size(), 2);
    // Test reaching max capacity.
    cache.insert(3, 3);
    assert_eq!(cache.size(), 2);
    assert_eq!(cache.get(&1), Some(&3));
    assert_eq!(cache.get(&3), Some(&3));
    assert_eq!(cache.get(&2), None);
    // Test ttl functionality.
    cache.gc(SystemTime::now()
        .checked_add(Duration::from_secs(10))
        .unwrap());
    assert_eq!(cache.size(), 0);
}

#[test]
fn test_get_transaction_by_hash() {
    let mut pool = setup_mempool().0;
    let db_sequence_number = 10;
    let txn = TestTransaction::new(0, db_sequence_number, 1).make_signed_transaction();
    pool.add_txn(
        txn.clone(),
        1,
        AccountSequenceInfo::Sequential(db_sequence_number),
        TimelineState::NotReady,
    );
    let hash = txn.clone().committed_hash();
    let ret = pool.get_by_hash(hash);
    assert_eq!(ret, Some(txn));

    let ret = pool.get_by_hash(HashValue::random());
    assert!(ret.is_none());
}

#[test]
fn test_get_transaction_by_hash_after_the_txn_is_updated() {
    let mut pool = setup_mempool().0;
    let db_sequence_number = 10;
    let txn = TestTransaction::new(0, db_sequence_number, 1).make_signed_transaction();
    pool.add_txn(
        txn.clone(),
        1,
        AccountSequenceInfo::Sequential(db_sequence_number),
        TimelineState::NotReady,
    );
    let hash = txn.committed_hash();

    // new txn with higher gas price
    let new_txn = TestTransaction::new(0, db_sequence_number, 100).make_signed_transaction();
    pool.add_txn(
        new_txn.clone(),
        1,
        AccountSequenceInfo::Sequential(db_sequence_number),
        TimelineState::NotReady,
    );
    let new_txn_hash = new_txn.clone().committed_hash();

    let txn_by_old_hash = pool.get_by_hash(hash);
    assert!(txn_by_old_hash.is_none());

    let txn_by_new_hash = pool.get_by_hash(new_txn_hash);
    assert_eq!(txn_by_new_hash, Some(new_txn));
}

#[test]
fn test_bytes_limit() {
    let mut config = NodeConfig::random();
    config.mempool.capacity = 100;
    let mut pool = CoreMempool::new(&config);
    // add 100 transacionts
    for seq in 0..100 {
        add_txn(&mut pool, TestTransaction::new(1, seq, 1)).unwrap();
    }
    let get_all = pool.get_batch(100, 100 * 1024, HashSet::new());
    assert_eq!(get_all.len(), 100);
    let txn_size = get_all[0].raw_txn_bytes_len() as u64;
    let limit = 10;
    let hit_limit = pool.get_batch(100, txn_size * limit, HashSet::new());
    assert_eq!(hit_limit.len(), limit as usize);
}