use anchor_lang::prelude::*;

// These seeds keep each pool and its LP mint at a predictable address.
#[constant]
pub const POOL_SEED: &[u8] = b"pool";
#[constant]
pub const LP_MINT_SEED: &[u8] = b"lp_mint";
#[constant]
pub const VAULT_A_SEED: &[u8] = b"vault_a";
#[constant]
pub const VAULT_B_SEED: &[u8] = b"vault_b";
#[constant]
pub const TREASURY_A_SEED: &[u8] = b"treasury_a";
#[constant]
pub const TREASURY_B_SEED: &[u8] = b"treasury_b";

// Fees use basis points, where 10,000 basis points equal 100 percent.
pub const BASIS_POINTS: u64 = 10_000;
pub const MAX_FEE_BPS: u16 = 1_000;
pub const LP_DECIMALS: u8 = 9;
