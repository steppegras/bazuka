use super::*;

use colored::Colorize;

pub async fn log_info<B: Blockchain>(
    context: Arc<RwLock<NodeContext<B>>>,
) -> Result<(), NodeError> {
    let ctx = context.read().await;
    let mut inf = Vec::new();
    inf.extend([
        ("Height", ctx.blockchain.get_height()?.to_string()),
        (
            "Outdated states",
            ctx.blockchain.get_outdated_contracts()?.len().to_string(),
        ),
        ("Timestamp", ctx.network_timestamp().to_string()),
        ("Node count", ctx.peer_manager.node_count().to_string()),
        ("Peer count", ctx.peer_manager.get_peers().len().to_string()),
    ]);

    inf.push(("Chain Pool", ctx.mempool.chain_sourced.len().to_string()));
    inf.push(("MPN Pool", ctx.mempool.mpn_sourced.len().to_string()));

    let wallet_addr = ctx.wallet.get_address();
    let balance = ctx
        .blockchain
        .get_balance(wallet_addr, crate::core::TokenId::Ziesha)?;
    inf.push(("Balance", balance.to_string()));

    println!(
        "{}",
        inf.into_iter()
            .map(|(k, v)| format!("{}: {}", k.bright_blue(), v))
            .collect::<Vec<String>>()
            .join(" ")
    );

    Ok(())
}
