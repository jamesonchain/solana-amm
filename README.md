# Simple Constant Product AMM

This is a small Anchor program that holds two SPL tokens in a pool and issues LP tokens to people who add liquidity. It supports deposits, withdrawals, and swaps, with a fee and treasury accounts.

## How the formula works

In the formula x * y = k, x and y are the amounts of each token in the pool, and k is their product. A swap uses the formula to choose an output amount, and fees can make the product grow as the pool earns value.

## Instructions

Initialize creates a pool for two token mints, an LP mint, two pool vaults, and a treasury token account for each mint. The mint addresses must be supplied in sorted order, and the total fee cannot be more than 1,000 basis points.

Deposit transfers token A and token B into the pool and mints LP tokens, subject to a minimum LP output. For later deposits, LP shares are calculated from the smaller proportional contribution, so provide amounts close to the pool ratio.

Withdraw burns LP tokens and returns the matching share of both pool reserves. It checks the minimum amount requested for each token.

Swap trades token A for token B or token B for token A, using the constant product formula and a minimum output check.

## Fees and treasury

The swap fee is taken from the input amount. One hundred basis points equals one percent, and the maximum fee is 1,000 basis points, or ten percent. Half of the rounded fee goes to the treasury account for the input token, rounded down, and the rest stays in the pool. The pool has one treasury account for each token because one token account can hold only one mint.

## Build and test

Install the JavaScript packages with `npm install`.

Build the Anchor program with `npm run build`.

Run the TypeScript LiteSVM tests with `npm test`. This command builds the program, checks the TypeScript, and runs the tests in memory.

The tests cover initialization, deposits, withdrawals, swaps in both directions, slippage failures, an excessive fee, and the treasury balance after a swap.

## What I learned

I learned that a pool keeps track of two token balances and that the swap formula uses both balances. I also learned that LP tokens represent a share of the pool, and fees can be split between the pool and a treasury.

This is a learning project and has not been audited. Do not use it to manage real funds.
