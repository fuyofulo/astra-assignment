# Sandwich Attack Simulator

A Rust program that simulates sandwich attacks on Pump.fun using local AMM state modeling.

## Features

- **Local AMM Simulation**: Models Pump.fun bonding curve mechanics without RPC calls
- **Sandwich Attack Demo**: Simulates complete front-run, victim, back-run sequence
- **Economic Analysis**: Shows extracted value, price impact, and bot profit calculations
- **Interactive Input**: Accepts victim SOL amount for customized simulations

## Usage

Run the program and enter a victim SOL input amount:

```bash
cargo run
Enter hypothetical victim SOL input (e.g., 1 for 1 SOL): 0.5
```

## Algorithm

1. **Baseline Calculation**: Simulates victim transaction without attack
2. **Front-run**: Bot buys tokens first, increasing price
3. **Victim Execution**: Victim buys at inflated price, experiencing slippage
4. **Back-run**: Bot sells in two phases - break-even and profit-taking

## Output

Shows detailed transaction sequence with:
- Token amounts and SOL values
- Price changes at each step
- Extracted value from victim
- Bot's net profit/loss per transaction
- Total attack profitability

## AMM Model

Uses Pump.fun's bonding curve formula:
- Virtual reserves: 30 SOL / 1.073B tokens initially
- Real reserves: 0 SOL / 793.1M tokens initially
- 30 BPS (0.3%) trading fee
- Constant product formula with fee deduction
