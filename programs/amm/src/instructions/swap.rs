use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

use crate::{
    constants::{BASIS_POINTS, POOL_SEED},
    error::AmmError,
    state::Pool,
};

// Both directions use the same validated pool, vaults, and token mints.
#[derive(Accounts)]
pub struct Swap<'info> {
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
        address = pool.treasury_a,
        token::mint = mint_a,
        token::authority = pool
    )]
    pub treasury_a: Account<'info, TokenAccount>,
    #[account(
        mut,
        address = pool.treasury_b,
        token::mint = mint_b,
        token::authority = pool
    )]
    pub treasury_b: Account<'info, TokenAccount>,
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

pub fn handle_swap(
    ctx: Context<Swap>,
    amount_in: u64,
    min_amount_out: u64,
    a_to_b: bool,
) -> Result<()> {
    require!(amount_in > 0, AmmError::ZeroAmount);

    // Select token accounts and reserves from the requested direction.
    let (source, input_vault, treasury, output_vault, destination, reserve_in, reserve_out) =
        if a_to_b {
            (
                &ctx.accounts.user_a,
                &ctx.accounts.vault_a,
                &ctx.accounts.treasury_a,
                &ctx.accounts.vault_b,
                &ctx.accounts.user_b,
                ctx.accounts.vault_a.amount,
                ctx.accounts.vault_b.amount,
            )
        } else {
            (
                &ctx.accounts.user_b,
                &ctx.accounts.vault_b,
                &ctx.accounts.treasury_b,
                &ctx.accounts.vault_a,
                &ctx.accounts.user_a,
                ctx.accounts.vault_b.amount,
                ctx.accounts.vault_a.amount,
            )
        };
    require!(
        reserve_in > 0 && reserve_out > 0,
        AmmError::InsufficientLiquidity
    );

    // Apply the full swap fee to the quote, then send half of it to treasury.
    let fee = u128::from(amount_in)
        .checked_mul(u128::from(ctx.accounts.pool.fee_bps))
        .ok_or(AmmError::MathOverflow)?
        .checked_div(u128::from(BASIS_POINTS))
        .ok_or(AmmError::MathOverflow)?;
    let fee = u64::try_from(fee).map_err(|_| AmmError::MathOverflow)?;
    let protocol_fee = fee.checked_div(2).ok_or(AmmError::MathOverflow)?;
    let effective_input = amount_in.checked_sub(fee).ok_or(AmmError::MathOverflow)?;

    // The constant product quote uses the input after the full fee.
    let numerator = u128::from(reserve_out)
        .checked_mul(u128::from(effective_input))
        .ok_or(AmmError::MathOverflow)?;
    let denominator = u128::from(reserve_in)
        .checked_add(u128::from(effective_input))
        .ok_or(AmmError::MathOverflow)?;
    let amount_out = numerator
        .checked_div(denominator)
        .ok_or(AmmError::MathOverflow)?;
    let amount_out = u64::try_from(amount_out).map_err(|_| AmmError::MathOverflow)?;
    require!(amount_out > 0, AmmError::InsufficientLiquidity);
    require!(amount_out >= min_amount_out, AmmError::SlippageExceeded);

    // Move the input into the pool before sending the protocol share out.
    token::transfer(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            Transfer {
                from: source.to_account_info(),
                to: input_vault.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        ),
        amount_in,
    )?;

    // The pool PDA pays the treasury and the user's output account.
    let bump = [ctx.accounts.pool.bump];
    let signer_seeds: &[&[u8]] = &[
        POOL_SEED,
        ctx.accounts.pool.mint_a.as_ref(),
        ctx.accounts.pool.mint_b.as_ref(),
        &bump,
    ];
    if protocol_fee > 0 {
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.key(),
                Transfer {
                    from: input_vault.to_account_info(),
                    to: treasury.to_account_info(),
                    authority: ctx.accounts.pool.to_account_info(),
                },
                &[signer_seeds],
            ),
            protocol_fee,
        )?;
    }
    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            Transfer {
                from: output_vault.to_account_info(),
                to: destination.to_account_info(),
                authority: ctx.accounts.pool.to_account_info(),
            },
            &[signer_seeds],
        ),
        amount_out,
    )?;

    Ok(())
}
