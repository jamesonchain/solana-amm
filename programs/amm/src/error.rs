use anchor_lang::prelude::*;

#[error_code]
pub enum AmmError {
    #[msg("The token mints must be different and ordered by address")]
    InvalidMintOrder,
    #[msg("The fee is above the allowed maximum")]
    FeeTooHigh,
    #[msg("Amounts must be greater than zero")]
    ZeroAmount,
    #[msg("The pool does not have enough liquidity")]
    InsufficientLiquidity,
    #[msg("The requested output is below the minimum")]
    SlippageExceeded,
    #[msg("The pool has an invalid balance or LP supply")]
    InvalidPoolState,
    #[msg("A checked math operation failed")]
    MathOverflow,
}
