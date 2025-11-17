use crate::parser::pumpfun::{ParsedTransaction, TradeType};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct SandwichDetection {
    pub victim: ParsedTransaction,
    pub frontruns: Vec<ParsedTransaction>,
    pub backruns: Vec<ParsedTransaction>,
    pub net_profit_sol: i64,
    pub net_token_delta: i64,
}

#[derive(Debug, Clone)]
pub struct FrontRunEvent {
    pub victim: ParsedTransaction,
    pub frontruns: Vec<ParsedTransaction>,
}

#[derive(Debug, Clone)]
pub struct BackRunEvent {
    pub victim: ParsedTransaction,
    pub backruns: Vec<ParsedTransaction>,
}

#[derive(Debug, Clone, Default)]
pub struct DetectionSummary {
    pub front_runs: Vec<FrontRunEvent>,
    pub back_runs: Vec<BackRunEvent>,
    pub sandwiches: Vec<SandwichDetection>,
}

#[derive(Debug, Clone)]
pub struct DetectorConfig {
    pub max_slot_gap: u64,
    pub min_victim_abs_sol: f64,
    pub min_victim_abs_token: f64,
    pub min_profit_lamports: i64,
    pub min_bot_trades: usize,
}

impl Default for DetectorConfig {
    fn default() -> Self {
        Self {
            max_slot_gap: 3,
            min_victim_abs_sol: 0.01,
            min_victim_abs_token: 100_000_000.0,  
            min_profit_lamports: 10_000,
            min_bot_trades: 2, 
        }
    }
}

pub fn detect_wide_attacks(trades: &[ParsedTransaction], cfg: &DetectorConfig) -> DetectionSummary {
    if trades.is_empty() {
        return DetectionSummary::default();
    }

    let mut signer_counts: HashMap<String, usize> = HashMap::new();
    for tx in trades {
        *signer_counts.entry(tx.signer.clone()).or_default() += 1;
    }
    let bot_signers: HashSet<String> = signer_counts
        .into_iter()
        .filter_map(|(signer, count)| (count >= cfg.min_bot_trades).then_some(signer))
        .collect();

    let mut by_slot: BTreeMap<u64, Vec<ParsedTransaction>> = BTreeMap::new();
    for tx in trades.iter().cloned() {
        by_slot.entry(tx.slot).or_default().push(tx);
    }

    let slot_keys: Vec<u64> = by_slot.keys().cloned().collect();
    let mut summary = DetectionSummary::default();

    for &slot in &slot_keys {
        let Some(current) = by_slot.get(&slot) else {
            continue;
        };

        for victim in current.iter() {
            let execution = analyze_execution(victim);
            if !execution.any() {
                continue;
            }

            if !magnitude_exceeds(victim, cfg) {
                continue;
            }

            let start_slot = slot.saturating_sub(cfg.max_slot_gap);
            let end_slot = slot.saturating_add(cfg.max_slot_gap);

            let mut frontruns: Vec<ParsedTransaction> = Vec::new();
            for (&prev_slot, txs) in by_slot.range(start_slot..=slot) {
                for tx in txs {
                    if tx.signature == victim.signature {
                        continue;
                    }
                    if tx.mint != victim.mint {
                        continue;
                    }
                    if prev_slot == slot && !occurs_before(tx, victim) {
                        continue;
                    }
                    if !bot_signers.contains(&tx.signer) {
                        continue;
                    }
                    if is_frontrun_candidate(tx, victim) {
                        frontruns.push(tx.clone());
                    }
                }
            }

            if !frontruns.is_empty() {
                summary.front_runs.push(FrontRunEvent {
                    victim: victim.clone(),
                    frontruns: frontruns.clone(),
                });
            }

            let mut backruns: Vec<ParsedTransaction> = Vec::new();
            for (&next_slot, txs) in by_slot.range(slot..=end_slot) {
                for tx in txs {
                    if tx.signature == victim.signature {
                        continue;
                    }
                    if tx.mint != victim.mint {
                        continue;
                    }
                    if next_slot == slot && !occurs_after(tx, victim) {
                        continue;
                    }
                    if !bot_signers.contains(&tx.signer) {
                        continue;
                    }
                    if is_backrun_candidate(tx, victim) {
                        backruns.push(tx.clone());
                    }
                }
            }

            if !backruns.is_empty() {
                summary.back_runs.push(BackRunEvent {
                    victim: victim.clone(),
                    backruns: backruns.clone(),
                });
            }

            let mut net_sol: i64 = 0;
            let mut net_tokens: i64 = 0;
            for tx in frontruns.iter().chain(backruns.iter()) {
                net_sol += tx.sol_change;
                net_tokens += tx.token_change;
            }

            if !frontruns.is_empty() && !backruns.is_empty() {
                if net_sol >= cfg.min_profit_lamports {
                    summary.sandwiches.push(SandwichDetection {
                        victim: victim.clone(),
                        frontruns: frontruns.clone(),
                        backruns: backruns.clone(),
                        net_profit_sol: net_sol,
                        net_token_delta: net_tokens,
                    });
                }
            }
        }
    }

    summary
}

fn is_frontrun_candidate(front: &ParsedTransaction, victim: &ParsedTransaction) -> bool {
    occurs_before(front, victim) && front.trade_type == victim.trade_type
}

fn is_backrun_candidate(back: &ParsedTransaction, victim: &ParsedTransaction) -> bool {
    occurs_after(back, victim) && back.trade_type != victim.trade_type
}

fn occurs_before(a: &ParsedTransaction, b: &ParsedTransaction) -> bool {
    (a.slot < b.slot) || (a.slot == b.slot && a.signature < b.signature)
}

fn occurs_after(a: &ParsedTransaction, b: &ParsedTransaction) -> bool {
    (a.slot > b.slot) || (a.slot == b.slot && a.signature > b.signature)
}

#[derive(Clone, Copy, Debug)]
struct ExecutionBreach {
    price_limit: bool,
    amount_limit: bool,
}

impl ExecutionBreach {
    fn any(self) -> bool {
        self.price_limit || self.amount_limit
    }
}

fn analyze_execution(tx: &ParsedTransaction) -> ExecutionBreach {
    match tx.trade_type {
        TradeType::Buy => {
            let actual_spent = negative_amount(tx.sol_change);
            let tokens_received = positive_amount(tx.token_change);
            ExecutionBreach {
                price_limit: actual_spent > tx.sol_limit_specified,
                amount_limit: tokens_received < tx.token_amount_requested,
            }
        }
        TradeType::Sell => {
            let sol_received = positive_amount(tx.sol_change);
            let tokens_sold = negative_amount(tx.token_change);
            ExecutionBreach {
                price_limit: sol_received < tx.sol_limit_specified,
                amount_limit: tokens_sold > tx.token_amount_requested,
            }
        }
    }
}

fn magnitude_exceeds(tx: &ParsedTransaction, cfg: &DetectorConfig) -> bool {
    tx.sol_change.abs_as_sol() >= cfg.min_victim_abs_sol
        || (tx.token_change as f64).abs() >= cfg.min_victim_abs_token
}

fn positive_amount(value: i64) -> u64 {
    if value > 0 { value as u64 } else { 0 }
}

fn negative_amount(value: i64) -> u64 {
    if value < 0 { (-value) as u64 } else { 0 }
}

pub trait LamportsExt {
    fn abs_as_sol(&self) -> f64;
    fn as_sol(&self) -> f64;
}

impl LamportsExt for i64 {
    fn abs_as_sol(&self) -> f64 {
        (*self as f64).abs() / 1_000_000_000.0
    }

    fn as_sol(&self) -> f64 {
        *self as f64 / 1_000_000_000.0
    }
}
