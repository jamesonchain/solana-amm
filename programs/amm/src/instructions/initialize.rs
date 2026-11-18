use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

use crate::{
    constants::{
        LP_DECIMALS, LP_MINT_SEED, MAX_FEE_BPS, POOL_SEED, TREASURY_A_SEED, TREASURY_B_SEED,
        VAULT_A_SEED, VAULT_B_SEED,
    },
    error::AmmError,
    state::Pool,
};

// Create the pool state, LP mint, two reserve vaults, and two fee accounts.
#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    pub mint_a: Account<'info, Mint>,
    pub mint_b: Account<'info, Mint>,
    #[account(
        init,
        payer = payer,
        space = 8 + Pool::INIT_SPACE,
        seeds = [POOL_SEED, mint_a.key().as_ref(), mint_b.key().as_ref()],
        bump
    )]
    pub pool: Account<'info, Pool>,
    #[account(
        init,
        payer = payer,
        seeds = [LP_MINT_SEED, pool.key().as_ref()],
        bump,
        mint::decimals = LP_DECIMALS,
        mint::authority = pool
    )]
    pub lp_mint: Account<'info, Mint>,
    #[account(
        init,
        payer = payer,
        seeds = [VAULT_A_SEED, pool.key().as_ref()],
        bump,
        token::mint = mint_a,
        token::authority = pool
    )]
    pub vault_a: Account<'info, TokenAccount>,
    #[account(
        init,
        payer = payer,
        seeds = [VAULT_B_SEED, pool.key().as_ref()],
        bump,
        token::mint = mint_b,
        token::authority = pool
    )]
    pub vault_b: Account<'info, TokenAccount>,
    #[account(
        init,
        payer = payer,
        seeds = [TREASURY_A_SEED, pool.key().as_ref()],
        bump,
        token::mint = mint_a,
        token::authority = pool
    )]
    pub treasury_a: Account<'info, TokenAccount>,
    #[account(
        init,
        payer = payer,
        seeds = [TREASURY_B_SEED, pool.key().as_ref()],
        bump,
        token::mint = mint_b,
        token::authority = pool
    )]
    pub treasury_b: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn handle_initialize(ctx: Context<Initialize>, fee_bps: u16) -> Result<()> {
    // Keep mint order unique so the same pair cannot be initialized backwards.
    require!(
        ctx.accounts.mint_a.key() < ctx.accounts.mint_b.key(),
        AmmError::InvalidMintOrder
    );
    require!(fee_bps <= MAX_FEE_BPS, AmmError::FeeTooHigh);

    // Save the validated pool configuration for later instructions.
    let pool = &mut ctx.accounts.pool;
    pool.mint_a = ctx.accounts.mint_a.key();
    pool.mint_b = ctx.accounts.mint_b.key();
    pool.lp_mint = ctx.accounts.lp_mint.key();
    pool.vault_a = ctx.accounts.vault_a.key();
    pool.vault_b = ctx.accounts.vault_b.key();
    pool.treasury_a = ctx.accounts.treasury_a.key();
    pool.treasury_b = ctx.accounts.treasury_b.key();
    pool.fee_bps = fee_bps;
    pool.bump = ctx.bumps.pool;

    Ok(())
}
