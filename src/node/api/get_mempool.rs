use super::messages::{GetMempoolRequest, GetMempoolResponse};
use super::{NodeContext, NodeError};
use crate::blockchain::Blockchain;
use crate::core::{ChainSourcedTx, TransactionData};
use std::sync::Arc;
use tokio::sync::RwLock;

pub async fn get_mempool<B: Blockchain>(
    context: Arc<RwLock<NodeContext<B>>>,
    _req: GetMempoolRequest,
) -> Result<GetMempoolResponse, NodeError> {
    let context = context.read().await;
    let mpn_contract_id = context.blockchain.config().mpn_contract_id;
    Ok(GetMempoolResponse {
        chain_sourced: context
            .mempool
            .chain_sourced
            .clone()
            .into_keys()
            .filter(|tx| {
                // Do not share MPN txs with others! It's a competetion :)
                if let ChainSourcedTx::TransactionAndDelta(tx) = tx {
                    if let TransactionData::UpdateContract { contract_id, .. } = &tx.tx.data {
                        contract_id != &mpn_contract_id
                    } else {
                        true
                    }
                } else {
                    true
                }
            })
            .collect(),
        mpn_sourced: context.mempool.mpn_sourced.clone().into_keys().collect(),
    })
}
