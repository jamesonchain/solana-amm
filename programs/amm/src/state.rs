use anchor_lang::prelude::*;

// The pool records the accounts and fee used by every AMM instruction.
#[account]
#[derive(InitSpace)]
pub struct Pool {
    pub mint_a: Pubkey,
    pub mint_b: Pubkey,
    pub lp_mint: Pubkey,
    pub vault_a: Pubkey,
    pub vault_b: Pubkey,
    pub treasury_a: Pubkey,
    pub treasury_b: Pubkey,
    pub fee_bps: u16,
    pub bump: u8,
}
