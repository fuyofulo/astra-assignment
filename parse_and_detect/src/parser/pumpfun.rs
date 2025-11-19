use borsh::BorshDeserialize;
use bs58;
use solana_transaction_status::{
    EncodedConfirmedTransactionWithStatusMeta, EncodedTransaction, UiInnerInstructions,
    UiInstruction, UiMessage, UiParsedInstruction, UiParsedMessage, UiTransactionStatusMeta,
    UiTransactionTokenBalance,
};

const BUY_DISCRIMINATOR: [u8; 8] = [102, 6, 61, 18, 1, 218, 235, 234];
const SELL_DISCRIMINATOR: [u8; 8] = [51, 230, 133, 164, 1, 127, 131, 173];

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum TradeType {
    Buy,
    Sell,
}

#[derive(Debug, Clone)]
pub struct ParsedTransaction {
    pub signature: String,
    pub slot: u64,
    pub signer: String,
    pub mint: String,
    pub trade_type: TradeType,
    pub token_amount_requested: u64,
    pub sol_limit_specified: u64,
    pub sol_change: i64,
    pub token_change: i64,
}

#[derive(BorshDeserialize, Debug)]
struct BuyArgs {
    pub amount: u64,
    pub max_sol_cost: u64,
}

#[derive(BorshDeserialize, Debug)]
struct SellArgs {
    pub amount: u64,
    pub min_sol_output: u64,
}

pub fn parse_transaction(
    tx: &EncodedConfirmedTransactionWithStatusMeta,
    signature: &str,
    mint_address: &str,
) -> Option<ParsedTransaction> {
    let message = match &tx.transaction.transaction {
        EncodedTransaction::Json(tx_json) => match &tx_json.message {
            UiMessage::Parsed(message) => message,
            _ => return None,
        },
        _ => return None,
    };

    let signer = message.account_keys.first()?.pubkey.clone();
    let slot = tx.slot;

    let mut decoded = scan_instruction_stream(message.instructions.iter().enumerate());

    if decoded.is_none() {
        if let Some(meta) = &tx.transaction.meta {
            if let Some(inner_groups) = meta.inner_instructions.as_slice() {
                for UiInnerInstructions {
                    index: _index,
                    instructions,
                } in inner_groups
                {
                    for (_inner_idx, instruction) in instructions.iter().enumerate() {
                        if let Some(hit) = decode_pump_instruction(instruction) {
                            decoded = Some(hit);
                            break;
                        }
                    }
                    if decoded.is_some() {
                        break;
                    }
                }
            }
        }
    }

    match decoded {
        Some(decoded) => {
            let (sol_change, token_change) = tx
                .transaction
                .meta
                .as_ref()
                .map(|meta| {
                    (
                        compute_sol_change(meta, message, &signer).unwrap_or(0),
                        compute_token_change(meta, &signer, mint_address).unwrap_or(0),
                    )
                })
                .unwrap_or((0, 0));

            println!("----------");
            println!("signature: {}", signature);
            println!("signer: {}", signer);
            println!("mint: {}", mint_address);
            println!(
                "wanted: {:?} {} tokens (SOL limit {})",
                decoded.trade_type, decoded.token_amount_requested, decoded.sol_limit_specified
            );
            println!("executed: ΔSOL {} | Δtoken {}", sol_change, token_change);

            match decoded.trade_type {
                TradeType::Buy => {
                    let actual_sol_spent = if sol_change < 0 { -sol_change } else { 0 };
                    let tokens_received = if token_change > 0 { token_change } else { 0 };

                    println!("BUY IMPACT:");
                    if actual_sol_spent > decoded.sol_limit_specified as i64 {
                        let overpaid = actual_sol_spent - decoded.sol_limit_specified as i64;
                        println!("  Overpaid by {} lamports ({:.6} SOL) - limit breached!",
                                overpaid, overpaid as f64 / 1_000_000_000.0);
                    } else {
                        println!("  SOL spend within limit");
                    }
                    if tokens_received < decoded.token_amount_requested as i64 {
                        let shortage = decoded.token_amount_requested as i64 - tokens_received;
                        println!("  Got {} fewer tokens than requested!",
                                shortage);
                    } else {
                        println!("  Received requested token amount");
                    }
                }
                TradeType::Sell => {
                    let actual_sol_received = if sol_change > 0 { sol_change } else { 0 };
                    let tokens_sold = if token_change < 0 { -token_change } else { 0 };

                    println!("SELL IMPACT:");
                    if actual_sol_received < decoded.sol_limit_specified as i64 {
                        let underpaid = decoded.sol_limit_specified as i64 - actual_sol_received;
                        println!("  Received {} fewer lamports than expected ({:.6} SOL shortfall)!",
                                underpaid, underpaid as f64 / 1_000_000_000.0);
                    } else {
                        println!("  SOL received meets expectation");
                    }
                    if tokens_sold > decoded.token_amount_requested as i64 {
                        let oversold = tokens_sold - decoded.token_amount_requested as i64;
                        println!("  Sold {} more tokens than planned!",
                                oversold);
                    } else {
                        println!("  Sold planned token amount");
                    }
                }
            }
            println!("----------");

            Some(ParsedTransaction {                signature: signature.to_string(),
                slot,
                signer,
                mint: mint_address.to_string(),
                trade_type: decoded.trade_type,
                token_amount_requested: decoded.token_amount_requested,
                sol_limit_specified: decoded.sol_limit_specified,
                sol_change,
                token_change,
            })
        }
        None => None,
    }
}

fn scan_instruction_stream<'a, I>(iter: I) -> Option<DecodedInstruction>
where
    I: Iterator<Item = (usize, &'a UiInstruction)>,
{
    for (_idx, instruction) in iter {
        if let Some(decoded) = decode_pump_instruction(instruction) {
            return Some(decoded);
        }
    }
    None
}


struct DecodedInstruction {
    trade_type: TradeType,
    token_amount_requested: u64,
    sol_limit_specified: u64,
}

fn decode_pump_instruction(instruction: &UiInstruction) -> Option<DecodedInstruction> {
    match instruction {
        UiInstruction::Compiled(compiled) => decode_instruction_data(&compiled.data),
        UiInstruction::Parsed(parsed) => match parsed {
            UiParsedInstruction::PartiallyDecoded(partial) => {
                decode_instruction_data(&partial.data)
            }
            UiParsedInstruction::Parsed(_parsed_instruction) => None,
        },
    }
}

fn decode_instruction_data(data_b58: &str) -> Option<DecodedInstruction> {
    let raw = bs58::decode(data_b58).into_vec().ok()?;
    if raw.len() < 8 {
        return None;
    }

    try_decode(&raw[..8], &raw[8..]).or_else(|| {
        if raw.len() >= 9 {
            try_decode(&raw[1..9], &raw[9..])
        } else {
            None
        }
    })
}

fn try_decode(disc_slice: &[u8], payload: &[u8]) -> Option<DecodedInstruction> {
    let disc: [u8; 8] = disc_slice.try_into().ok()?;

    if disc == BUY_DISCRIMINATOR {
        let args = BuyArgs::try_from_slice(payload).ok()?;
        return Some(DecodedInstruction {
            trade_type: TradeType::Buy,
            token_amount_requested: args.amount,
            sol_limit_specified: args.max_sol_cost,
        });
    }

    if disc == SELL_DISCRIMINATOR {
        let args = SellArgs::try_from_slice(payload).ok()?;
        return Some(DecodedInstruction {
            trade_type: TradeType::Sell,
            token_amount_requested: args.amount,
            sol_limit_specified: args.min_sol_output,
        });
    }

    None
}

use solana_transaction_status::option_serializer::OptionSerializer;

trait OptionSerializerExt<T> {
    fn as_slice(&self) -> Option<&[T]>;
}

impl<T> OptionSerializerExt<T> for OptionSerializer<Vec<T>> {
    fn as_slice(&self) -> Option<&[T]> {
        match self {
            OptionSerializer::Some(values) => Some(values.as_slice()),
            OptionSerializer::Skip | OptionSerializer::None => None,
        }
    }
}

fn compute_sol_change(
    meta: &UiTransactionStatusMeta,
    message: &UiParsedMessage,
    signer: &str,
) -> Option<i64> {
    let account_index = message
        .account_keys
        .iter()
        .position(|account| account.pubkey == signer)?;
    let pre = *meta.pre_balances.get(account_index)? as i128;
    let post = *meta.post_balances.get(account_index)? as i128;
    Some(i128_to_i64(post - pre))
}

fn compute_token_change(meta: &UiTransactionStatusMeta, owner: &str, mint: &str) -> Option<i64> {
    let pre = extract_token_total(meta.pre_token_balances.as_slice(), owner, mint);
    let post = extract_token_total(meta.post_token_balances.as_slice(), owner, mint);

    if pre.is_none() && post.is_none() {
        return None;
    }

    let delta = post.unwrap_or(0) - pre.unwrap_or(0);
    Some(i128_to_i64(delta))
}

fn extract_token_total(
    balances: Option<&[UiTransactionTokenBalance]>,
    owner: &str,
    mint: &str,
) -> Option<i128> {
    let mut total: i128 = 0;
    let mut found = false;

    if let Some(entries) = balances {
        for balance in entries {
            if balance.mint != mint {
                continue;
            }

            let balance_owner = match balance.owner.as_ref() {
                OptionSerializer::Some(owner_str) => owner_str,
                OptionSerializer::Skip | OptionSerializer::None => continue,
            };

            if balance_owner == owner {
                if let Ok(amount) = balance.ui_token_amount.amount.parse::<i128>() {
                    total += amount;
                    found = true;
                }
            }
        }
    }

    if found { Some(total) } else { None }
}

fn i128_to_i64(value: i128) -> i64 {
    if value > i64::MAX as i128 {
        i64::MAX
    } else if value < i64::MIN as i128 {
        i64::MIN
    } else {
        value as i64
    }
}
