# Pump.fun Sandwich Attack Detector

A Rust program that indexes raw transactions and detects sandwich attacks on Pump.fun transactions for a token mint.

## Features

- **Transaction Indexing**: Fetches and indexes recent Solana transactions by token mint
- **Instruction Parsing**: Decodes Pump.fun buy/sell instructions from raw transaction data
- **Attack Detection**: Identifies victims with unfavorable execution (price slippage, insufficient tokens)
- **Pattern Analysis**: Detects front-run, back-run, and complete sandwich attack patterns
- **Impact Assessment**: Analyzes attack impact showing overpayment and token shortages

## Usage

```bash
cargo run <TOKEN_MINT_ADDRESS>
```

Example:
```bash
cargo run GX5AhAvBYUSyguNhbokH9dkn3xxWnYCC6E4AxgiEUdFs
```

## Output

- **Parser**: Shows what each transaction wanted vs. what it executed, with attack impact analysis
- **Detection**: Categorizes attacks into front-runs, back-runs, and sandwiches with profit calculations

## Configuration

Detection thresholds are configurable in `DetectorConfig`:
- Minimum trade size for victim consideration
- Slot gap limits for attack windows
- Minimum bot trading frequency
- Profit thresholds for sandwich classification
