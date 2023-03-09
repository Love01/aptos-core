// Copyright © Aptos Foundation
// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use crate::db_options::{gen_state_kv_cfds, state_kv_db_column_families};
use anyhow::Result;
use aptos_config::config::RocksdbConfigs;
use aptos_rocksdb_options::gen_rocksdb_options;
use aptos_schemadb::DB;
use arr_macro::arr;
use std::{path::Path, sync::Arc};

pub const STATE_KV_DB_NAME: &str = "state_kv_db";
pub const STATE_KV_METADATA_DB_NAME: &str = "state_kv_metadata_db";

pub struct StateKvDb {
    state_kv_metadata_db: Arc<DB>,
    state_kv_db_shards: [Arc<DB>; 256],
}

impl StateKvDb {
    // TODO(grao): Support more flexible path to make it easier for people to put different shards
    // on different disks.
    pub fn open<P: AsRef<Path>>(
        db_root_path: P,
        rocksdb_configs: RocksdbConfigs,
        readonly: bool,
        ledger_db: Arc<DB>,
    ) -> Result<Self> {
        if !rocksdb_configs.use_state_kv_db {
            return Ok(Self {
                state_kv_metadata_db: Arc::clone(&ledger_db),
                state_kv_db_shards: arr![Arc::clone(&ledger_db); 256],
            });
        }

        let state_kv_metadata_db_path = db_root_path
            .as_ref()
            .join(STATE_KV_DB_NAME)
            .join("metadata");

        let state_kv_metadata_db = Arc::new(if readonly {
            DB::open_cf_readonly(
                &gen_rocksdb_options(&rocksdb_configs.state_kv_db_config, true),
                state_kv_metadata_db_path.clone(),
                STATE_KV_METADATA_DB_NAME,
                state_kv_db_column_families(),
            )?
        } else {
            DB::open_cf(
                &gen_rocksdb_options(&rocksdb_configs.state_kv_db_config, false),
                state_kv_metadata_db_path.clone(),
                STATE_KV_METADATA_DB_NAME,
                gen_state_kv_cfds(&rocksdb_configs.state_kv_db_config),
            )?
        });

        Ok(Self {
            state_kv_metadata_db,
            state_kv_db_shards: arr![Arc::clone(&ledger_db); 256],
        })
    }
}
