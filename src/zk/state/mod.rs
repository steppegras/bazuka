use thiserror::Error;

use super::*;
use crate::core::ContractId;
use crate::crypto::jubjub;
use crate::db::{keys, KvStore, KvStoreError, RamKvStore, WriteOp};
use ff::Field;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;

#[derive(Error, Debug)]
pub enum StateManagerError {
    #[error("kvstore error happened: {0}")]
    KvStoreError(#[from] KvStoreError),
    #[error("locator error: {0}")]
    LocatorError(#[from] ZkLocatorError),
    #[error("contract not found")]
    ContractNotFound,
    #[error("not locating a scalar")]
    NonScalarLocatorError,
    #[error("locator parse error: {0}")]
    LocatorParseError(#[from] ParseZkDataLocatorError),
    #[error("not locating a tree")]
    NonTreeLocatorError,
    #[error("zk error: {0}")]
    ZkError(#[from] ZkError),
}

#[derive(Clone)]
pub struct KvStoreStateManager<H: ZkHasher> {
    _hasher: std::marker::PhantomData<H>,
}

pub struct ZkStateBuilder<H: ZkHasher> {
    contract_id: ContractId,
    _hasher: std::marker::PhantomData<H>,
    db: RamKvStore,
}

impl<H: ZkHasher> ZkStateBuilder<H> {
    pub fn new(state_model: ZkStateModel) -> Self {
        let contract_id = ContractId::from_str(
            "0000000000000000000000000000000000000000000000000000000000000000",
        )
        .unwrap();
        let mut db = RamKvStore::new();
        db.update(&[WriteOp::Put(
            keys::contract(&contract_id),
            ZkContract {
                initial_state: ZkCompressedState::empty::<H>(state_model.clone()),
                state_model,
                deposit_functions: vec![],
                withdraw_functions: vec![],
                functions: vec![],
            }
            .into(),
        )])
        .unwrap();
        Self {
            contract_id,
            db,
            _hasher: std::marker::PhantomData,
        }
    }
    pub fn batch_set(&mut self, delta: &ZkDeltaPairs) -> Result<(), StateManagerError> {
        let height = KvStoreStateManager::<H>::height_of(&self.db, self.contract_id)?;
        KvStoreStateManager::<H>::update_contract(
            &mut self.db,
            self.contract_id,
            delta,
            height + 1,
        )?;
        Ok(())
    }
    pub fn get(&mut self, loc: ZkDataLocator) -> Result<ZkScalar, StateManagerError> {
        KvStoreStateManager::<H>::get_data(&self.db, self.contract_id, &loc)
    }
    pub fn compress(self) -> Result<ZkCompressedState, StateManagerError> {
        KvStoreStateManager::<H>::root(&self.db, self.contract_id)
    }

    pub fn prove(
        &self,
        tree_loc: ZkDataLocator,
        ind: u64,
    ) -> Result<Vec<[ZkScalar; 3]>, StateManagerError> {
        KvStoreStateManager::<H>::prove(&self.db, self.contract_id, tree_loc, ind)
    }
}

impl<H: ZkHasher> KvStoreStateManager<H> {
    pub fn get_mpn_account<K: KvStore>(
        db: &K,
        mpn_contract_id: ContractId,
        index: u64,
    ) -> Result<MpnAccount, StateManagerError> {
        let cells = (0..3)
            .map(|i| Self::get_data(db, mpn_contract_id, &ZkDataLocator(vec![index, i as u64])))
            .collect::<Result<Vec<ZkScalar>, StateManagerError>>()?;
        let mut token_indices = HashSet::new();
        for (k, _) in db.pairs(keys::local_value(
            &mpn_contract_id,
            &ZkDataLocator(vec![index, 3]),
            true,
        ))? {
            let loc = ZkDataLocator::from_str(k.0.split('-').nth(3).unwrap())?;
            if loc.0.len() == 4 {
                token_indices.insert(loc.0[2]);
            }
        }
        let mut tokens = HashMap::new();
        for i in token_indices {
            let tok = Self::get_data(
                db,
                mpn_contract_id,
                &ZkDataLocator(vec![index, 3, i as u64, 0]),
            )?;
            let bal = Self::get_data(
                db,
                mpn_contract_id,
                &ZkDataLocator(vec![index, 3, i as u64, 1]),
            )?;
            let tok_is_zero: bool = tok.is_zero().into();
            if !tok_is_zero {
                tokens.insert(i, Money::new(tok.into(), bal.try_into()?));
            }
        }
        Ok(MpnAccount {
            nonce: cells[0].try_into()?,
            address: jubjub::PointAffine(cells[1], cells[2]),
            tokens,
        })
    }

    pub fn get_mpn_accounts<K: KvStore>(
        db: &K,
        mpn_contract_id: ContractId,
        page: usize,
        page_size: usize,
    ) -> Result<Vec<(u64, MpnAccount)>, StateManagerError> {
        let mut indices = Vec::new();
        for (k, _) in db.pairs(keys::local_scalar_value_prefix(&mpn_contract_id).into())? {
            let loc = ZkDataLocator::from_str(k.0.split('-').nth(3).unwrap())?;
            indices.push(loc.0[0]);
        }
        indices.sort_unstable();
        indices.dedup();
        let mut accs = Vec::new();
        for ind in indices.into_iter().skip(page_size * page).take(page_size) {
            accs.push((
                ind,
                KvStoreStateManager::<H>::get_mpn_account::<K>(db, mpn_contract_id, ind)?,
            ));
        }
        Ok(accs)
    }

    pub fn set_mpn_account<K: KvStore>(
        db: &mut K,
        mpn_contract_id: ContractId,
        index: u64,
        acc: MpnAccount,
        size_diff: &mut u64,
    ) -> Result<(), StateManagerError> {
        let vals = [acc.nonce.into(), acc.address.0, acc.address.1];
        vals.into_iter()
            .enumerate()
            .map(|(i, val)| {
                Self::set_data(
                    db,
                    mpn_contract_id,
                    ZkDataLocator(vec![index, i as u64]),
                    val,
                    size_diff,
                )
            })
            .collect::<Result<Vec<ZkScalar>, StateManagerError>>()?;
        for (ind, money) in acc.tokens.iter() {
            Self::set_data(
                db,
                mpn_contract_id,
                ZkDataLocator(vec![index, 3, *ind as u64, 0]),
                money.token_id.into(),
                size_diff,
            )?;
            Self::set_data(
                db,
                mpn_contract_id,
                ZkDataLocator(vec![index, 3, *ind as u64, 1]),
                ZkScalar::from(money.amount),
                size_diff,
            )?;
        }
        Ok(())
    }

    pub fn delete_contract<K: KvStore>(
        db: &mut K,
        id: ContractId,
    ) -> Result<(), StateManagerError> {
        let mut rems = Vec::new();
        for (k, _) in db.pairs(keys::local_prefix(&id).into())? {
            rems.push(WriteOp::Remove(k));
        }
        db.update(&rems)?;
        Ok(())
    }

    pub fn height_of<K: KvStore>(db: &K, id: ContractId) -> Result<u64, StateManagerError> {
        if let Some(blob) = db.get(keys::local_height(&id))? {
            Ok(blob.try_into()?)
        } else {
            Ok(0)
        }
    }

    pub fn prove<K: KvStore>(
        db: &K,
        id: ContractId,
        tree_loc: ZkDataLocator,
        mut curr_ind: u64,
    ) -> Result<Vec<[ZkScalar; 3]>, StateManagerError> {
        let loc_type = Self::type_of(db, id)?.locate(&tree_loc)?;
        if let ZkStateModel::List {
            log4_size,
            item_type,
        } = loc_type
        {
            let mut default_value = item_type.compress_default::<H>();
            let mut proof = Vec::new();

            for layer in (0..log4_size).rev() {
                let mut proof_part = [ZkScalar::default(); 3];
                let aux_offset = ((1 << (2 * (layer + 1))) - 1) / 3;
                let start = curr_ind - (curr_ind % 4);
                let mut i = 0;
                for leaf_index in start..start + 4 {
                    if leaf_index != curr_ind {
                        proof_part[i] = if layer == log4_size - 1 {
                            Self::get_data(db, id, &tree_loc.index(leaf_index as u64))?
                        } else {
                            match db.get(keys::local_tree_aux(
                                &id,
                                &tree_loc,
                                aux_offset + leaf_index,
                            ))? {
                                Some(b) => b.try_into()?,
                                None => default_value,
                            }
                        };
                        i += 1;
                    };
                }
                curr_ind /= 4;
                default_value = H::hash(&[default_value; 4]);
                proof.push(proof_part);
            }

            Ok(proof)
        } else {
            Err(StateManagerError::NonTreeLocatorError)
        }
    }

    pub fn type_of<K: KvStore>(db: &K, id: ContractId) -> Result<ZkStateModel, StateManagerError> {
        let cont: ZkContract = db
            .get(keys::contract(&id))?
            .ok_or(StateManagerError::ContractNotFound)?
            .try_into()?;
        Ok(cont.state_model)
    }

    pub fn root<K: KvStore>(
        db: &K,
        id: ContractId,
    ) -> Result<ZkCompressedState, StateManagerError> {
        if let Some(blob) = db.get(keys::local_root(&id))? {
            Ok(blob.try_into()?)
        } else {
            Ok(ZkCompressedState::empty::<H>(Self::type_of(db, id)?))
        }
    }

    pub fn rollback_contract<K: KvStore>(
        db: &mut K,
        id: ContractId,
    ) -> Result<Option<ZkCompressedState>, StateManagerError> {
        let mut fork = db.mirror();
        let mut root = Self::root(&fork, id)?;
        let height = Self::height_of(&fork, id)?;
        let rollback_key = keys::local_rollback_to_height(&id, height);
        let rollback_patch = if let Some(patch) = Self::rollback_of(&fork, id, 1)? {
            patch
        } else {
            return Ok(None);
        };
        for (k, v) in rollback_patch.0 {
            root.state_hash = Self::set_data(
                &mut fork,
                id,
                k,
                v.unwrap_or_default(),
                &mut root.state_size,
            )?;
        }
        fork.update(&[
            WriteOp::Remove(rollback_key),
            WriteOp::Put(keys::local_root(&id), root.into()),
            WriteOp::Put(keys::local_height(&id), (height - 1).into()),
        ])?;

        db.update(&fork.to_ops())?;

        Ok(Some(root))
    }

    pub fn delta_of<K: KvStore>(
        db: &K,
        id: ContractId,
        away: u64,
    ) -> Result<Option<ZkDeltaPairs>, StateManagerError> {
        let mut data = ZkDeltaPairs(Default::default());
        for i in 0..away {
            if let Some(rollback) = Self::rollback_of(db, id, i + 1)? {
                for (k, _) in rollback.0 {
                    data.0.insert(k.clone(), Some(Self::get_data(db, id, &k)?));
                }
            } else {
                return Ok(None);
            }
        }
        Ok(Some(data))
    }

    pub fn rollback_of<K: KvStore>(
        db: &K,
        id: ContractId,
        away: u64,
    ) -> Result<Option<ZkDeltaPairs>, StateManagerError> {
        let height = Self::height_of(db, id)?;
        Ok(
            match db.get(keys::local_rollback_to_height(&id, height - away))? {
                Some(b) => Some(b.try_into()?),
                None => None,
            },
        )
    }

    pub fn get_full_state<K: KvStore>(
        db: &K,
        id: ContractId,
    ) -> Result<ZkState, StateManagerError> {
        const MAX_ROLLBACKS: u64 = 5;
        let mut data = ZkDataPairs(Default::default());
        for (k, v) in db.pairs(keys::local_scalar_value_prefix(&id).into())? {
            let loc = ZkDataLocator::from_str(k.0.split('-').nth(3).unwrap())?;
            data.0.insert(loc, v.try_into()?);
        }
        let mut rollbacks = Vec::<ZkDeltaPairs>::new();
        let height = Self::height_of(db, id)?;
        for i in 0..MAX_ROLLBACKS {
            if height > i {
                rollbacks.push(
                    match db.get(keys::local_rollback_to_height(&id, height - i - 1))? {
                        Some(b) => b.try_into()?,
                        None => {
                            break;
                        }
                    },
                );
            } else {
                break;
            }
        }
        Ok(ZkState { data, rollbacks })
    }

    pub fn reset_contract<K: KvStore>(
        db: &mut K,
        id: ContractId,
        height: u64,
        state: &ZkState,
        expected_delta_targets: &[ZkCompressedState],
    ) -> Result<(ZkCompressedState, Vec<ZkCompressedState>), StateManagerError> {
        let mut fork = db.mirror();
        let contract_type = Self::type_of(&fork, id)?;
        for (k, _) in fork.pairs(keys::local_prefix(&id).into())? {
            fork.update(&[WriteOp::Remove(k)])?;
        }

        let mut state_hash = contract_type.compress_default::<H>();
        let mut state_size = 0;
        for (k, v) in state.data.0.iter() {
            state_hash = Self::set_data(&mut fork, id, k.clone(), *v, &mut state_size)?;
        }

        fork.update(&[
            WriteOp::Put(
                keys::local_root(&id),
                ZkCompressedState {
                    state_hash,
                    state_size,
                }
                .into(),
            ),
            WriteOp::Put(keys::local_height(&id), height.into()),
        ])?;

        let root = Self::root(&fork, id)?;

        let mut rollback_results = Vec::new();
        let mut rollback_ops = Vec::new();
        let mut rollback_fork = fork.mirror();
        let mut rollback_root = root;

        for (i, (rollback, expected)) in state
            .rollbacks
            .iter()
            .zip(expected_delta_targets.iter())
            .enumerate()
        {
            for (k, v) in &rollback.0 {
                rollback_root.state_hash = Self::set_data(
                    &mut rollback_fork,
                    id,
                    k.clone(),
                    v.unwrap_or_default(),
                    &mut rollback_root.state_size,
                )?;
            }
            if rollback_root == *expected {
                rollback_ops.push(WriteOp::Put(
                    keys::local_rollback_to_height(&id, height - 1 - i as u64),
                    rollback.into(),
                ));
                rollback_results.push(rollback_root);
                log::debug!("Delta accepted!");
            } else {
                log::debug!("Delta rejected!");
                break;
            }
        }
        fork.update(&rollback_ops)?;

        db.update(&fork.to_ops())?;

        Ok((root, rollback_results))
    }

    pub fn update_contract<K: KvStore>(
        db: &mut K,
        id: ContractId,
        patch: &ZkDeltaPairs,
        target_height: u64,
    ) -> Result<(), StateManagerError> {
        const MAX_ROLLBACKS: u64 = 5;
        let mut rollback_patch = ZkDeltaPairs(HashMap::new());
        let mut fork = db.mirror();
        let mut root = Self::root(&fork, id)?;
        for (k, v) in &patch.0 {
            let prev_val = Self::get_data(&fork, id, k)?;
            rollback_patch.0.insert(k.clone(), Some(prev_val)); // Or None if default
            root.state_hash = Self::set_data(
                &mut fork,
                id,
                k.clone(),
                v.unwrap_or_default(),
                &mut root.state_size,
            )?;
        }
        fork.update(&[
            WriteOp::Put(keys::local_root(&id), root.into()),
            WriteOp::Put(
                keys::local_rollback_to_height(&id, target_height - 1),
                (&rollback_patch).into(),
            ),
            WriteOp::Put(keys::local_height(&id), target_height.into()),
        ])?;
        if target_height > MAX_ROLLBACKS {
            fork.update(&[WriteOp::Remove(keys::local_rollback_to_height(
                &id,
                target_height - 1 - MAX_ROLLBACKS,
            ))])?;
        }
        db.update(&fork.to_ops())?;
        Ok(())
    }

    pub fn set_data<K: KvStore>(
        db: &mut K,
        id: ContractId,
        mut locator: ZkDataLocator,
        mut value: ZkScalar,
        size_diff: &mut u64,
    ) -> Result<ZkScalar, StateManagerError> {
        let contract_type = Self::type_of(db, id)?;
        let mut ops = Vec::new();

        if contract_type.locate(&locator)? != ZkStateModel::Scalar {
            return Err(StateManagerError::NonScalarLocatorError);
        }

        let prev_is_zero: bool = Self::get_data(db, id, &locator)?.is_zero().into();

        ops.push(if value.is_zero().into() {
            if !prev_is_zero {
                *size_diff -= 1;
            }
            WriteOp::Remove(keys::local_value(&id, &locator, true))
        } else {
            if prev_is_zero {
                *size_diff += 1;
            }
            WriteOp::Put(keys::local_value(&id, &locator, true), value.into())
        });

        while let Some(curr_loc) = locator.0.pop() {
            let curr_type = contract_type.locate(&locator)?;
            match curr_type.clone() {
                ZkStateModel::List {
                    item_type,
                    log4_size,
                } => {
                    let leaf_index = curr_loc;
                    let mut curr_ind = leaf_index;
                    let mut default_value = item_type.compress_default::<H>();
                    for layer in (0..log4_size).rev() {
                        let mut dats = Vec::new();
                        let aux_offset = ((1 << (2 * (layer + 1))) - 1) / 3;
                        let start = curr_ind - (curr_ind % 4);
                        for leaf_index in start..start + 4 {
                            dats.push(if leaf_index == curr_ind {
                                value
                            } else if layer == log4_size - 1 {
                                let mut full_loc = locator.clone();
                                full_loc.0.push(leaf_index as u64);
                                Self::get_data(db, id, &full_loc)?
                            } else {
                                match db.get(keys::local_tree_aux(
                                    &id,
                                    &locator,
                                    aux_offset + leaf_index,
                                ))? {
                                    Some(b) => b.try_into()?,
                                    None => default_value,
                                }
                            });
                        }

                        value = H::hash(&dats);
                        default_value = H::hash(&[default_value; 4]);

                        curr_ind /= 4;

                        if layer > 0 {
                            let parent_aux_offset = ((1 << (2 * layer)) - 1) / 3;
                            let parent_index = parent_aux_offset + curr_ind;
                            let aux_key = keys::local_tree_aux(&id, &locator, parent_index);
                            ops.push(if value == default_value {
                                WriteOp::Remove(aux_key)
                            } else {
                                WriteOp::Put(aux_key, value.into())
                            });
                        }
                    }
                }
                ZkStateModel::Struct { field_types } => {
                    let mut dats = Vec::new();
                    for field_index in 0..field_types.len() {
                        dats.push(if field_index as u64 == curr_loc {
                            value
                        } else {
                            let mut full_loc = locator.clone();
                            full_loc.0.push(field_index as u64);
                            Self::get_data(db, id, &full_loc)?
                        });
                    }
                    value = H::hash(&dats);
                }
                ZkStateModel::Scalar => {
                    panic!()
                }
            }

            ops.push(if value == curr_type.compress_default::<H>() {
                WriteOp::Remove(keys::local_value(&id, &locator, false))
            } else {
                WriteOp::Put(keys::local_value(&id, &locator, false), value.into())
            });
        }

        db.update(&ops)?;
        Ok(value)
    }

    pub fn get_data<K: KvStore>(
        db: &K,
        cid: ContractId,
        locator: &ZkDataLocator,
    ) -> Result<ZkScalar, StateManagerError> {
        let sub_type = Self::type_of(db, cid)?.locate(locator)?;
        Ok(
            match db.get(keys::local_value(
                &cid,
                locator,
                sub_type == ZkStateModel::Scalar,
            ))? {
                Some(b) => b.try_into()?,
                None => sub_type.compress_default::<H>(),
            },
        )
    }
}
