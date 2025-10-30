use anchor_lang::prelude::*;

#[event]
pub struct DepositEvent {
    pub user: Pubkey,
    pub amount: u64,
    pub mint: Pubkey,
    pub vault: Pubkey,
}

#[event]
pub struct UnbondEvent {
    pub user: Pubkey,
    pub amount: u64,
    pub mint: Pubkey,
    pub vault: Pubkey,
}

#[event]
pub struct RedeemEvent {
    pub user: Pubkey,
    pub amount: u64,
    pub mint: Pubkey,
    pub vault: Pubkey,
}

#[event]
pub struct UnbondingPeriodUpdated {
    pub admin: Pubkey,
    pub old_period: i64,
    pub new_period: i64,
    pub mint: Pubkey,
    pub vault: Pubkey,
}

#[event]
pub struct RewardsPublished {
    pub admin: Pubkey,
    pub amount: u64,
    pub mint_program: Pubkey,
    pub vault_token_account: Pubkey,
    pub mint: Pubkey,
    pub vault: Pubkey
}
