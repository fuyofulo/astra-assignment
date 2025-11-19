use solana_client::rpc_client::{GetConfirmedSignaturesForAddress2Config, RpcClient};
use solana_client::rpc_config::RpcTransactionConfig;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use solana_transaction_status::UiTransactionEncoding;
use std::env;
use std::str::FromStr;
use dotenvy::dotenv;

mod detect;
mod parser;
use detect::{DetectorConfig, LamportsExt, detect_wide_attacks};
use parser::pumpfun::TradeType;

fn main() {
    dotenv().ok();

    let args: Vec<String> = env::args().collect();
    let mint_address_str = args.get(1).expect("Add a token mint address arg!");
    let mint_address = Pubkey::from_str(mint_address_str).expect("Invalid Address");

    let api_key = env::var("HELIUS_API_KEY").expect("HELIUS_API_KEY must be set in .env file");
    let rpc_url = format!("https://mainnet.helius-rpc.com/?api-key={}", api_key);
    let client = RpcClient::new(rpc_url.to_string());

    let signatures_config = GetConfirmedSignaturesForAddress2Config {
        limit: Some(50),
        before: None,
        until: None,
        commitment: None,
    };

    let mut parsed_trades: Vec<parser::pumpfun::ParsedTransaction> = Vec::new();

    let signatures = client
        .get_signatures_for_address_with_config(&mint_address, signatures_config)
        .unwrap();

    println!(
        "Found {} signatures. Fetching transactions...",
        signatures.len()
    );

    for tx_info in signatures {
        let signature = Signature::from_str(&tx_info.signature).unwrap();

        let config = RpcTransactionConfig {
            encoding: Some(UiTransactionEncoding::JsonParsed),
            max_supported_transaction_version: Some(0),
            commitment: None,
        };

        match client.get_transaction_with_config(&signature, config) {
            Ok(tx) => {
                let result = parser::pumpfun::parse_transaction(
                    &tx,
                    &signature.to_string(),
                    mint_address_str,
                );

                if let Some(parsed_tx) = result {
                    parsed_trades.push(parsed_tx);
                }
            }
            Err(e) => eprintln!("Failed {}: {}", signature, e),
        }
    }

    println!(
        "Successfully parsed {} pump.fun trades.",
        parsed_trades.len()
    );
    println!("need to do sandwich attack analysis now");

    let config = DetectorConfig::default();
    let summary = detect_wide_attacks(&parsed_trades, &config);

    println!("---- Detection Summary ----");
    println!("Total trades parsed: {}", parsed_trades.len());
    println!("Wide front-run candidates: {}", summary.front_runs.len());
    println!("Wide back-run candidates: {}", summary.back_runs.len());
    println!("Wide sandwich candidates: {}", summary.sandwiches.len());

    if !summary.front_runs.is_empty() {
        println!("\n-- Front-run Events --");
        for (idx, event) in summary.front_runs.iter().enumerate() {
            println!(
                "#{:02} Victim {} | slot {} | {} | ΔSOL {:+.4} SOL | Δtoken {} | Wanted: {} tokens (SOL limit {})",
                idx + 1,
                short_sig(&event.victim.signature),
                event.victim.slot,
                trade_badge(event.victim.trade_type),
                event.victim.sol_change.as_sol(),
                event.victim.token_change,
                event.victim.token_amount_requested,
                event.victim.sol_limit_specified
            );
            println!("Impact:{}", format_attack_impact(&event.victim));
            for (leg_idx, fr) in event.frontruns.iter().enumerate() {
                println!(
                    "FR{:02} [{}] slot {} signer {} | ΔSOL {:+.4} SOL | Δtoken {}",
                    leg_idx + 1,
                    trade_badge(fr.trade_type),
                    fr.slot,
                    short_sig(&fr.signer),
                    fr.sol_change.as_sol(),
                    fr.token_change
                );
            }
        }
    }

    if !summary.back_runs.is_empty() {
        println!("\n-- Back-run Events --");
        for (idx, event) in summary.back_runs.iter().enumerate() {
            println!(
                "#{:02} Victim {} | slot {} | {} | ΔSOL {:+.4} SOL | Δtoken {} | Wanted: {} tokens (SOL limit {})",
                idx + 1,
                short_sig(&event.victim.signature),
                event.victim.slot,
                trade_badge(event.victim.trade_type),
                event.victim.sol_change.as_sol(),
                event.victim.token_change,
                event.victim.token_amount_requested,
                event.victim.sol_limit_specified
            );
            println!("Impact:{}", format_attack_impact(&event.victim));
            for (leg_idx, br) in event.backruns.iter().enumerate() {
                println!(
                    "BR{:02} [{}] slot {} signer {} | ΔSOL {:+.4} SOL | Δtoken {}",
                    leg_idx + 1,
                    trade_badge(br.trade_type),
                    br.slot,
                    short_sig(&br.signer),
                    br.sol_change.as_sol(),
                    br.token_change
                );
            }
        }
    }

    if !summary.sandwiches.is_empty() {
        println!("\n-- Sandwich Events --");
        for (idx, det) in summary.sandwiches.iter().enumerate() {
            println!(
                "#{} Victim {} @ slot {} | {} | ΔSOL {:+.4} SOL | Δtoken {} | Wanted: {} tokens (SOL limit {})",
                idx + 1,
                short_sig(&det.victim.signature),
                det.victim.slot,
                trade_badge(det.victim.trade_type),
                det.victim.sol_change.as_sol(),
                det.victim.token_change,
                det.victim.token_amount_requested,
                det.victim.sol_limit_specified
            );
            println!("Impact:{}", format_attack_impact(&det.victim));
            println!("Frontruns: {}", det.frontruns.len());
            println!("Backruns: {}", det.backruns.len());
            println!(
                "Profit (SOL): {:.6}, net tokens {}",
                det.net_profit_sol.abs_as_sol(),
                det.net_token_delta
            );
            for (leg_idx, fr) in det.frontruns.iter().enumerate() {
                println!(
                    "FR{:02} [{}] slot {} signer {} | ΔSOL {:+.4} SOL | Δtoken {}",
                    leg_idx + 1,
                    trade_badge(fr.trade_type),
                    fr.slot,
                    short_sig(&fr.signer),
                    fr.sol_change.as_sol(),
                    fr.token_change
                );
            }
            for (leg_idx, br) in det.backruns.iter().enumerate() {
                println!(
                    "BR{:02} [{}] slot {} signer {} | ΔSOL {:+.4} SOL | Δtoken {}",
                    leg_idx + 1,
                    trade_badge(br.trade_type),
                    br.slot,
                    short_sig(&br.signer),
                    br.sol_change.as_sol(),
                    br.token_change
                );
            }
            println!();
        }
    }
}

fn short_sig(sig: &str) -> String {
    if sig.len() <= 8 {
        sig.to_string()
    } else {
        format!("{}…{}", &sig[..4], &sig[sig.len() - 4..])
    }
}

fn trade_badge(trade: TradeType) -> &'static str {
    match trade {
        TradeType::Buy => "BUY",
        TradeType::Sell => "SELL",
    }
}

fn format_attack_impact(tx: &parser::pumpfun::ParsedTransaction) -> String {
    let mut impact = String::new();

    match tx.trade_type {
        TradeType::Buy => {
            let actual_sol_spent = if tx.sol_change < 0 { -tx.sol_change } else { 0 };
            let tokens_received = if tx.token_change > 0 { tx.token_change } else { 0 };

            if actual_sol_spent > tx.sol_limit_specified as i64 {
                let overpaid = actual_sol_spent - tx.sol_limit_specified as i64;
                impact.push_str(&format!("OVERPAID {:.6} SOL", overpaid as f64 / 1_000_000_000.0));
            }
            if tokens_received < tx.token_amount_requested as i64 {
                let shortage = tx.token_amount_requested as i64 - tokens_received;
                impact.push_str(&format!("GOT {} FEWER TOKENS", shortage));
            }
        }
        TradeType::Sell => {
            let actual_sol_received = if tx.sol_change > 0 { tx.sol_change } else { 0 };
            let tokens_sold = if tx.token_change < 0 { -tx.token_change } else { 0 };

            if actual_sol_received < tx.sol_limit_specified as i64 {
                let underpaid = tx.sol_limit_specified as i64 - actual_sol_received;
                impact.push_str(&format!("RECEIVED {:.6} SOL LESS", underpaid as f64 / 1_000_000_000.0));
            }
            if tokens_sold > tx.token_amount_requested as i64 {
                let oversold = tokens_sold - tx.token_amount_requested as i64;
                impact.push_str(&format!("SOLD {} MORE TOKENS", oversold));
            }
        }
    }

    if impact.is_empty() {
        "FAIR EXECUTION".to_string()
    } else {
        impact
    }
}
