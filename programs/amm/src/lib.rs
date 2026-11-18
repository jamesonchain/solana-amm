pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use error::*;
pub use instructions::*;
pub use state::*;

declare_id!("93udxGjKRxgszietSgakLG5d55wqihCJ8mav2tDquaDL");

#[program]
pub mod amm {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, fee_bps: u16) -> Result<()> {
        crate::instructions::initialize::handle_initialize(ctx, fee_bps)
    }

    pub fn deposit(
        ctx: Context<Deposit>,
        amount_a: u64,
        amount_b: u64,
        min_lp_out: u64,
    ) -> Result<()> {
        crate::instructions::deposit::handle_deposit(ctx, amount_a, amount_b, min_lp_out)
    }

    pub fn withdraw(
        ctx: Context<Withdraw>,
        amount_lp: u64,
        min_a_out: u64,
        min_b_out: u64,
    ) -> Result<()> {
        crate::instructions::withdraw::handle_withdraw(ctx, amount_lp, min_a_out, min_b_out)
    }

    pub fn swap(
        ctx: Context<Swap>,
        amount_in: u64,
        min_amount_out: u64,
        a_to_b: bool,
    ) -> Result<()> {
        crate::instructions::swap::handle_swap(ctx, amount_in, min_amount_out, a_to_b)
    }
}
