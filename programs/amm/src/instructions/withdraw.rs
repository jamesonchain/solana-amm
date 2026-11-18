use anchor_lang::prelude::*;
use anchor_spl::token::{self, Burn, Mint, Token, TokenAccount, Transfer};

use crate::{constants::POOL_SEED, error::AmmError, state::Pool};

// Verify the LP tokens belong to the signer and the vaults belong to this pool.
#[derive(Accounts)]
pub struct Withdraw<'info> {
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
        token::mint = lp_mint,
        token::authority = user
    )]
    pub user_lp: Account<'info, TokenAccount>,
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
    pub token_program: Program<'info, Token>,
}

pub fn handle_withdraw(
    ctx: Context<Withdraw>,
    amount_lp: u64,
    min_a_out: u64,
    min_b_out: u64,
) -> Result<()> {
    require!(amount_lp > 0, AmmError::ZeroAmount);
    require!(
        amount_lp <= ctx.accounts.user_lp.amount,
        AmmError::InsufficientLiquidity
    );

    let reserve_a = ctx.accounts.vault_a.amount;
    let reserve_b = ctx.accounts.vault_b.amount;
    let lp_supply = ctx.accounts.lp_mint.supply;
    require!(
        reserve_a > 0 && reserve_b > 0 && lp_supply > 0,
        AmmError::InvalidPoolState
    );

    // Calculate each output as the same fraction of its reserve as LP burned.
    let amount_a_out = u128::from(amount_lp)
        .checked_mul(u128::from(reserve_a))
        .ok_or(AmmError::MathOverflow)?
        .checked_div(u128::from(lp_supply))
        .ok_or(AmmError::InvalidPoolState)?;
    let amount_b_out = u128::from(amount_lp)
        .checked_mul(u128::from(reserve_b))
        .ok_or(AmmError::MathOverflow)?
        .checked_div(u128::from(lp_supply))
        .ok_or(AmmError::InvalidPoolState)?;
    let amount_a_out = u64::try_from(amount_a_out).map_err(|_| AmmError::MathOverflow)?;
    let amount_b_out = u64::try_from(amount_b_out).map_err(|_| AmmError::MathOverflow)?;

    require!(
        amount_a_out > 0 && amount_b_out > 0,
        AmmError::InsufficientLiquidity
    );
    require!(
        amount_a_out >= min_a_out && amount_b_out >= min_b_out,
        AmmError::SlippageExceeded
    );

    // Burn LP shares before returning their proportional reserves.
    token::burn(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            Burn {
                mint: ctx.accounts.lp_mint.to_account_info(),
                from: ctx.accounts.user_lp.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        ),
        amount_lp,
    )?;

    // The pool PDA signs both transfers from its reserve vaults.
    let bump = [ctx.accounts.pool.bump];
    let signer_seeds: &[&[u8]] = &[
        POOL_SEED,
        ctx.accounts.pool.mint_a.as_ref(),
        ctx.accounts.pool.mint_b.as_ref(),
        &bump,
    ];
    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            Transfer {
                from: ctx.accounts.vault_a.to_account_info(),
                to: ctx.accounts.user_a.to_account_info(),
                authority: ctx.accounts.pool.to_account_info(),
            },
            &[signer_seeds],
        ),
        amount_a_out,
    )?;
    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            Transfer {
                from: ctx.accounts.vault_b.to_account_info(),
                to: ctx.accounts.user_b.to_account_info(),
                authority: ctx.accounts.pool.to_account_info(),
            },
            &[signer_seeds],
        ),
        amount_b_out,
    )?;

    Ok(())
}
