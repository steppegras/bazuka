use crate::core::{
    Address, Amount, ContractDeposit, ContractId, ContractUpdate, ContractWithdraw, Money,
    MpnAddress, MpnDeposit, MpnWithdraw, RegularSendEntry, Signature, Signer, Token, TokenId,
    Transaction, TransactionAndDelta, TransactionData, ZkSigner,
};
use crate::crypto::SignatureScheme;
use crate::crypto::ZkSignatureScheme;
use crate::zk;
use crate::zk::ZkHasher;

#[derive(Clone)]
pub struct TxBuilder {
    seed: Vec<u8>,
    private_key: <Signer as SignatureScheme>::Priv,
    zk_private_key: <ZkSigner as ZkSignatureScheme>::Priv,
    address: Address,
    zk_address: <ZkSigner as ZkSignatureScheme>::Pub,
}

impl TxBuilder {
    pub fn new(seed: &[u8]) -> Self {
        let (pk, sk) = Signer::generate_keys(seed);
        let (zk_pk, zk_sk) = ZkSigner::generate_keys(seed);
        Self {
            seed: seed.to_vec(),
            address: pk,
            zk_address: zk_pk,
            private_key: sk,
            zk_private_key: zk_sk,
        }
    }
    pub fn get_priv_key(&self) -> <Signer as SignatureScheme>::Priv {
        self.private_key.clone()
    }
    pub fn get_address(&self) -> Address {
        self.address.clone()
    }
    pub fn get_zk_address(&self) -> <ZkSigner as ZkSignatureScheme>::Pub {
        self.zk_address.clone()
    }
    pub fn sign(&self, bytes: &[u8]) -> <Signer as SignatureScheme>::Sig {
        Signer::sign(&self.private_key, bytes)
    }
    pub fn sign_tx(&self, tx: &mut Transaction) {
        let bytes = bincode::serialize(&tx).unwrap();
        tx.sig = Signature::Signed(Signer::sign(&self.private_key, &bytes));
    }
    pub fn create_transaction(
        &self,
        memo: String,
        dst: Address,
        amount: Money,
        fee: Money,
        nonce: u32,
    ) -> TransactionAndDelta {
        self.create_multi_transaction(memo, vec![RegularSendEntry { dst, amount }], fee, nonce)
    }
    pub fn create_token(
        &self,
        memo: String,
        name: String,
        symbol: String,
        supply: Amount,
        decimals: u8,
        minter: Option<Address>,
        fee: Money,
        nonce: u32,
    ) -> (TransactionAndDelta, TokenId) {
        let mut tx = Transaction {
            memo,
            src: Some(self.get_address()),
            data: TransactionData::CreateToken {
                token: Token {
                    name,
                    symbol,
                    minter,
                    supply,
                    decimals,
                },
            },
            nonce,
            fee,
            sig: Signature::Unsigned,
        };
        self.sign_tx(&mut tx);

        let token_id = TokenId::new(&tx);
        (
            TransactionAndDelta {
                tx,
                state_delta: None,
            },
            token_id,
        )
    }
    pub fn create_multi_transaction(
        &self,
        memo: String,
        entries: Vec<RegularSendEntry>,
        fee: Money,
        nonce: u32,
    ) -> TransactionAndDelta {
        let mut tx = Transaction {
            memo,
            src: Some(self.get_address()),
            data: TransactionData::RegularSend { entries },
            nonce,
            fee,
            sig: Signature::Unsigned,
        };
        self.sign_tx(&mut tx);
        TransactionAndDelta {
            tx,
            state_delta: None,
        }
    }
    pub fn create_mpn_transaction(
        &self,
        from_token_index: u64,
        to: MpnAddress,
        to_token_index: u64,
        amount: Money,
        fee_token_index: u64,
        fee: Money,
        nonce: u64,
    ) -> zk::MpnTransaction {
        let mut tx = zk::MpnTransaction {
            nonce,

            src_pub_key: self.get_zk_address(),
            dst_pub_key: to.pub_key,

            src_token_index: from_token_index,
            src_fee_token_index: fee_token_index,
            dst_token_index: to_token_index,

            amount,
            fee,
            sig: Default::default(),
        };
        tx.sign(&self.zk_private_key);
        tx
    }
    pub fn create_contract(
        &self,
        memo: String,
        contract: zk::ZkContract,
        initial_state: zk::ZkDataPairs,
        fee: Money,
        nonce: u32,
    ) -> TransactionAndDelta {
        let mut tx = Transaction {
            memo,
            src: Some(self.get_address()),
            data: TransactionData::CreateContract { contract },
            nonce,
            fee,
            sig: Signature::Unsigned,
        };
        self.sign_tx(&mut tx);
        TransactionAndDelta {
            tx,
            state_delta: Some(initial_state.as_delta()),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn call_function(
        &self,
        memo: String,
        contract_id: ContractId,
        function_id: u32,
        state_delta: zk::ZkDeltaPairs,
        next_state: zk::ZkCompressedState,
        proof: zk::ZkProof,
        exec_fee: Money,
        miner_fee: Money,
        nonce: u32,
    ) -> TransactionAndDelta {
        let (_, sk) = Signer::generate_keys(&self.seed);
        let mut tx = Transaction {
            memo,
            src: Some(self.get_address()),
            data: TransactionData::UpdateContract {
                contract_id,
                updates: vec![ContractUpdate::FunctionCall {
                    function_id,
                    next_state,
                    proof,
                    fee: exec_fee,
                }],
            },
            nonce,
            fee: miner_fee,
            sig: Signature::Unsigned,
        };
        let bytes = bincode::serialize(&tx).unwrap();
        tx.sig = Signature::Signed(Signer::sign(&sk, &bytes));
        TransactionAndDelta {
            tx,
            state_delta: Some(state_delta),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn deposit_mpn(
        &self,
        memo: String,
        contract_id: ContractId,
        to: MpnAddress,
        to_token_index: u64,
        nonce: u32,
        amount: Money,
        fee: Money,
    ) -> MpnDeposit {
        let mut calldata_builder =
            zk::ZkStateBuilder::<crate::core::ZkHasher>::new(zk::MPN_DEPOSIT_STATE_MODEL.clone());
        let pk = to.pub_key.0.decompress();
        calldata_builder
            .batch_set(&zk::ZkDeltaPairs(
                [
                    (zk::ZkDataLocator(vec![0]), Some(pk.0)),
                    (zk::ZkDataLocator(vec![1]), Some(pk.1)),
                ]
                .into(),
            ))
            .unwrap();
        let mut tx = ContractDeposit {
            memo,
            src: self.get_address(),
            contract_id,
            deposit_circuit_id: 0,
            calldata: calldata_builder.compress().unwrap().state_hash,
            nonce,
            amount,
            fee,
            sig: None,
        };
        let bytes = bincode::serialize(&tx).unwrap();
        tx.sig = Some(Signer::sign(&self.private_key, &bytes));
        MpnDeposit {
            zk_address: to.pub_key,
            zk_token_index: to_token_index,
            payment: tx,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn withdraw_mpn(
        &self,
        memo: String,
        contract_id: ContractId,
        nonce: u64,
        token_index: u64,
        amount: Money,
        fee_token_index: u64,
        fee: Money,
        to: <Signer as SignatureScheme>::Pub,
    ) -> MpnWithdraw {
        let mut tx = ContractWithdraw {
            memo,
            dst: to,
            contract_id,
            withdraw_circuit_id: 0,
            calldata: zk::ZkScalar::default(),
            amount,
            fee,
        };
        let sig = ZkSigner::sign(
            &self.zk_private_key,
            crate::core::ZkHasher::hash(&[tx.fingerprint(), zk::ZkScalar::from(nonce as u64)]),
        );
        let mut calldata_builder =
            zk::ZkStateBuilder::<crate::core::ZkHasher>::new(zk::MPN_WITHDRAW_STATE_MODEL.clone());
        let pk = self.get_zk_address().0.decompress();
        calldata_builder
            .batch_set(&zk::ZkDeltaPairs(
                [
                    (zk::ZkDataLocator(vec![0]), Some(pk.0)),
                    (zk::ZkDataLocator(vec![1]), Some(pk.1)),
                    (
                        zk::ZkDataLocator(vec![2]),
                        Some(zk::ZkScalar::from(nonce as u64)),
                    ),
                    (zk::ZkDataLocator(vec![3]), Some(sig.r.0)),
                    (zk::ZkDataLocator(vec![4]), Some(sig.r.1)),
                    (zk::ZkDataLocator(vec![5]), Some(sig.s)),
                ]
                .into(),
            ))
            .unwrap();
        tx.calldata = calldata_builder.compress().unwrap().state_hash;
        MpnWithdraw {
            zk_address: self.get_zk_address(),
            zk_token_index: token_index,
            zk_fee_token_index: fee_token_index,
            zk_nonce: nonce,
            zk_sig: sig,
            payment: tx,
        }
    }
}
