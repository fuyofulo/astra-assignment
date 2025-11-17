use std::cmp::max;
use std::io::{self, BufRead};

const LAMPORTS_PER_SOL: u64 = 1_000_000_000;
const TOKEN_DECIMALS: u64 = 1_000_000;
const INITIAL_VIRTUAL_SOL: u64 = 30 * LAMPORTS_PER_SOL;
const INITIAL_VIRTUAL_TOKEN: u64 = 1_073_000_000 * TOKEN_DECIMALS;
const INITIAL_REAL_SOL: u64 = 0;
const INITIAL_REAL_TOKEN: u64 = 793_100_000 * TOKEN_DECIMALS;
const FEE_BPS: u64 = 30;
const GAS_EST_PER_TX: u64 = 5_000;

#[derive(Debug, Clone)]
struct PumpAmmState {
    virtual_sol: u64,
    virtual_token: u64,
    real_sol: u64,
    real_token: u64,
}

impl PumpAmmState {
    fn new() -> Self {
        Self {
            virtual_sol: INITIAL_VIRTUAL_SOL,
            virtual_token: INITIAL_VIRTUAL_TOKEN,
            real_sol: INITIAL_REAL_SOL,
            real_token: INITIAL_REAL_TOKEN,
        }
    }

    fn get_price(&self) -> f64 {
        if self.virtual_token == 0 {
            0.0
        } else {
            (self.virtual_sol as f64) / (self.virtual_token as f64)
        }
    }

    fn simulate_buy(&mut self, sol_in: u64, min_tokens_out: u64) -> (u64, u64) {
        let fee = (sol_in * FEE_BPS / 10_000).max(1);
        let sol_in_after_fee = sol_in.saturating_sub(fee);

        let tokens_out = if self.virtual_sol == 0 {
            0
        } else {
            (sol_in_after_fee as u128 * self.virtual_token as u128 / (self.virtual_sol as u128 + sol_in_after_fee as u128)) as u64
        };

        let tokens_out = if tokens_out < min_tokens_out {
            0
        } else {
            tokens_out
        };

        if tokens_out > 0 {
            self.virtual_sol += sol_in_after_fee;
            self.virtual_token = self.virtual_token.saturating_sub(tokens_out);
            self.real_sol += sol_in;
            self.real_token = self.real_token.saturating_sub(tokens_out);
        }

        (tokens_out, sol_in)
    }

    fn simulate_sell(&mut self, tokens_in: u64, min_sol_out: u64) -> u64 {
        let fee = (tokens_in * FEE_BPS / 10_000).max(1);
        let tokens_in_after_fee = tokens_in.saturating_sub(fee);

        let sol_out = if self.virtual_token == 0 {
            0
        } else {
            (tokens_in_after_fee as u128 * self.virtual_sol as u128 / (self.virtual_token as u128 + tokens_in_after_fee as u128)) as u64
        };

        let sol_out = if sol_out < min_sol_out {
            0
        } else {
            sol_out
        };

        if sol_out > 0 {
            self.virtual_sol = self.virtual_sol.saturating_sub(sol_out);
            self.virtual_token += tokens_in_after_fee;
            self.real_sol = self.real_sol.saturating_sub(sol_out);
            self.real_token += tokens_in;
        }

        sol_out
    }
}

fn main() {
    println!("Enter hypothetical victim SOL input (e.g., 1 for 1 SOL): ");
    let stdin = io::stdin();
    let victim_sol_in_f = stdin.lock().lines().next().unwrap().unwrap().trim().parse::<f64>().unwrap();
    let victim_sol_in = (victim_sol_in_f * LAMPORTS_PER_SOL as f64) as u64;
    let victim_min_tokens = (victim_sol_in / 2) * TOKEN_DECIMALS / LAMPORTS_PER_SOL;

    let mut amm = PumpAmmState::new();
    let base_slot: u64 = 380_000_000;

    println!("\nHypothetical Victim TX: Buy with {:.3} SOL, min tokens {}", victim_sol_in_f, victim_min_tokens / TOKEN_DECIMALS);
    let mut no_attack_amm = amm.clone();
    let (victim_tokens_no_attack, victim_sol_no_attack) = no_attack_amm.simulate_buy(victim_sol_in, victim_min_tokens);
    println!("\nBaseline (No Attack): Tokens {} ({:.0} with dec) for {:.3} SOL", victim_tokens_no_attack, victim_tokens_no_attack as f64 / TOKEN_DECIMALS as f64, victim_sol_no_attack as f64 / LAMPORTS_PER_SOL as f64);

    let bot_front_sol = victim_sol_in / 5;
    let bot_min_tokens_front = 0;
    let (bot_tokens_bought, bot_sol_paid_front) = amm.simulate_buy(bot_front_sol, bot_min_tokens_front);
    println!("\nSlot n ({}): Bot Front-run Buy: Tokens {} for {:.3} SOL", base_slot, bot_tokens_bought as f64 / TOKEN_DECIMALS as f64, bot_front_sol as f64 / LAMPORTS_PER_SOL as f64);
    println!("Price after front-run: {:.12} SOL/token", amm.get_price());

    let (victim_tokens, victim_sol_paid) = amm.simulate_buy(victim_sol_in, victim_min_tokens);
    println!("\nSlot n+1 ({}): Victim Buy: Tokens {} for {:.3} SOL", base_slot + 1, victim_tokens as f64 / TOKEN_DECIMALS as f64, victim_sol_paid as f64 / LAMPORTS_PER_SOL as f64);
    println!("Price after victim: {:.12} SOL/token", amm.get_price());

    let extracted_value = max(0, victim_sol_paid as i64 - victim_sol_no_attack as i64) as u64;
    println!("Extracted Value: {:.6} SOL", extracted_value as f64 / LAMPORTS_PER_SOL as f64);

    let break_even_needed = bot_sol_paid_front + GAS_EST_PER_TX * 2;
    let tokens_to_sell_be = bot_tokens_bought / 2;
    let min_sol_be = break_even_needed / 2;
    let bot_back1_sol = amm.simulate_sell(tokens_to_sell_be, min_sol_be);
    let net_be = (bot_back1_sol as i64 - (bot_sol_paid_front as i64 / 2 + GAS_EST_PER_TX as i64)) as f64 / LAMPORTS_PER_SOL as f64;
    println!("\nSlot n+2 ({}): Back-run 1 (Break Even): Sell {} tokens, Received {:.6} SOL (Net: {:.6})", base_slot + 2, tokens_to_sell_be as f64 / TOKEN_DECIMALS as f64, bot_back1_sol as f64 / LAMPORTS_PER_SOL as f64, net_be);
    println!("Price after back-run 1: {:.12} SOL/token", amm.get_price());
    let remaining_tokens = bot_tokens_bought - tokens_to_sell_be;
    let min_sol_profit = 0;
    let bot_back2_sol = amm.simulate_sell(remaining_tokens, min_sol_profit);
    let net_profit = (bot_back2_sol as i64 - (bot_sol_paid_front as i64 / 2 + GAS_EST_PER_TX as i64)) as f64 / LAMPORTS_PER_SOL as f64;
    println!("\nSlot n+3 ({}): Back-run 2 (Profit): Sell {} tokens, Received {:.6} SOL (Net: {:.6})", base_slot + 3, remaining_tokens as f64 / TOKEN_DECIMALS as f64, bot_back2_sol as f64 / LAMPORTS_PER_SOL as f64, net_profit);
    println!("Price after back-run 2: {:.12} SOL/token", amm.get_price());

    let total_net = net_be + net_profit;
    println!("\nBot Total Net Profit: {:.6} SOL", total_net);
}