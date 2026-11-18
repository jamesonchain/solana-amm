use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, MintTo, Token, TokenAccount, Transfer};

use crate::{constants::POOL_SEED, error::AmmError, state::Pool};

// Check each token account against the pool before moving any tokens.
#[derive(Accounts)]
pub struct Deposit<'info> {
    pub user: Signer<'info>,
    #[account(
        seeds = [POOL_SEED, mint_a.key().as_ref(), mint_b.key().as_ref()],
        bump = pool.bump
    )]
    pub pool: Account<'info, Pool>,
    #[account(address = pool.mint_a)]
    pub mint_a: Account<'info, Mint>,
    #[account(address = pool.mint_b)]
    pub mint_b: Account<'info, Mint>,
    #[account(
        mut,
        address = pool.lp_mint,
        mint::authority = pool
    )]
    pub lp_mint: Account<'info, Mint>,
    #[account(
        mut,
        address = pool.vault_a,
        token::mint = mint_a,
        token::authority = pool
    )]
    pub vault_a: Account<'info, TokenAccount>,
    #[account(
        mut,
        address = pool.vault_b,
        token::mint = mint_b,
        token::authority = pool
    )]
    pub vault_b: Account<'info, TokenAccount>,
    #[account(
        mut,
        token::mint = mint_a,
        token::authority = user
    )]
    pub user_a: Account<'info, TokenAccount>,
    #[account(
        mut,
        token::mint = mint_b,
        token::authority = user
    )]
    pub user_b: Account<'info, TokenAccount>,
    #[account(
        mut,
        token::mint = lp_mint,
        token::authority = user
    )]
    pub user_lp: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

pub fn handle_deposit(
    ctx: Context<Deposit>,
    amount_a: u64,
    amount_b: u64,
    min_lp_out: u64,
) -> Result<()> {
    require!(amount_a > 0 && amount_b > 0, AmmError::ZeroAmount);

    let reserve_a = ctx.accounts.vault_a.amount;
    let reserve_b = ctx.accounts.vault_b.amount;
    let lp_supply = ctx.accounts.lp_mint.supply;

    // The first LP shares are the integer square root of the deposited product.
    let lp_out = if lp_supply == 0 {
        require!(reserve_a == 0 && reserve_b == 0, AmmError::InvalidPoolState);
        let product = u128::from(amount_a)
            .checked_mul(u128::from(amount_b))
            .ok_or(AmmError::MathOverflow)?;
        integer_sqrt(product)?
    } else {
        require!(reserve_a > 0 && reserve_b > 0, AmmError::InvalidPoolState);

        // Later deposits receive shares based on the limiting reserve ratio.
        let shares_a = u128::from(amount_a)
            .checked_mul(u128::from(lp_supply))
            .ok_or(AmmError::MathOverflow)?
            .checked_div(u128::from(reserve_a))
            .ok_or(AmmError::InvalidPoolState)?;
        let shares_b = u128::from(amount_b)
            .checked_mul(u128::from(lp_supply))
            .ok_or(AmmError::MathOverflow)?
            .checked_div(u128::from(reserve_b))
            .ok_or(AmmError::InvalidPoolState)?;
        u64::try_from(shares_a.min(shares_b)).map_err(|_| AmmError::MathOverflow)?
    };

    require!(lp_out > 0, AmmError::InsufficientLiquidity);
    require!(lp_out >= min_lp_out, AmmError::SlippageExceeded);

    // Transfer both assets into the pool after the share and slippage checks.
    token::transfer(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            Transfer {
                from: ctx.accounts.user_a.to_account_info(),
                to: ctx.accounts.vault_a.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        ),
        amount_a,
    )?;
    token::transfer(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            Transfer {
                from: ctx.accounts.user_b.to_account_info(),
                to: ctx.accounts.vault_b.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        ),
        amount_b,
    )?;

    // The pool PDA signs for the LP mint it controls.
    let bump = [ctx.accounts.pool.bump];
    let signer_seeds: &[&[u8]] = &[
        POOL_SEED,
        ctx.accounts.pool.mint_a.as_ref(),
        ctx.accounts.pool.mint_b.as_ref(),
        &bump,
    ];
    token::mint_to(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            MintTo {
                mint: ctx.accounts.lp_mint.to_account_info(),
                to: ctx.accounts.user_lp.to_account_info(),
                authority: ctx.accounts.pool.to_account_info(),
            },
            &[signer_seeds],
        ),
        lp_out,
    )?;

    Ok(())
}

// Use integer Newton iteration so initial shares never use floating point math.
fn integer_sqrt(value: u128) -> Result<u64> {
    if value == 0 {
        return Ok(0);
    }

    let mut estimate = value;
    let mut next = estimate
        .checked_add(1)
        .ok_or(AmmError::MathOverflow)?
        .checked_div(2)
        .ok_or(AmmError::MathOverflow)?;

    while next < estimate {
        estimate = next;
        next = estimate
            .checked_add(value.checked_div(estimate).ok_or(AmmError::MathOverflow)?)
            .ok_or(AmmError::MathOverflow)?
            .checked_div(2)
            .ok_or(AmmError::MathOverflow)?;
    }

    u64::try_from(estimate).map_err(|_| AmmError::MathOverflow.into())
}
