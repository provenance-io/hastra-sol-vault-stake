use anchor_lang::prelude::*;

#[error_code]
pub enum CustomErrorCode {
    #[msg("Invalid amount")]
    InvalidAmount = 1,
    #[msg("Invalid token received")]
    InvalidTokenReceived = 2,
    #[msg("Invalid vault")]
    InvalidVault = 3,
    #[msg("Invalid authority")]
    InvalidAuthority = 4,
    #[msg("Insufficient balance")]
    InsufficientBalance = 5,
    #[msg("Unbonding period not elapsed")]
    UnbondingPeriodNotElapsed = 6,
    #[msg("Insufficient unbonding balance")]
    InsufficientUnbondingBalance = 7,
    #[msg("Unbonding is currently in progress")]
    UnbondingInProgress = 8,
    
    #[msg("Invalid mint provided")]
    InvalidMint = 9,
    #[msg("Invalid vault mint provided")]
    InvalidVaultMint = 10,
    #[msg("Invalid ticket owner")]
    InvalidTicketOwner = 11,

    #[msg("Invalid mint authority")]
    InvalidMintAuthority = 12,
    #[msg("Insufficient vault balance")]
    InsufficientVaultBalance = 13,
    #[msg("Invalid vault authority")]
    InvalidVaultAuthority = 14,
    #[msg("Invalid freeze authority")]
    InvalidFreezeAuthority = 15,
    #[msg("ProgramData account did not match expected PDA.")]
    InvalidProgramData = 16,
    #[msg("Program has no upgrade authority (set to None).")]
    NoUpgradeAuthority = 17,
    #[msg("Signer is not the upgrade authority.")]
    InvalidUpgradeAuthority = 18,
    #[msg("Signer account missing.")]
    MissingSigner = 19,
    #[msg("Too many freeze administrators.")]
    TooManyAdministrators = 20,
    #[msg("Unauthorized freeze administrator")]
    UnauthorizedFreezeAdministrator = 21,
    #[msg("Invalid rewards epoch")]
    InvalidRewardsEpoch = 22,
    #[msg("Invalid merkle proof")]
    InvalidMerkleProof = 23,
    #[msg("Rewards already claimed for this epoch")]
    RewardsAlreadyClaimed = 24,
    #[msg("Invalid rewards administrator")]
    InvalidRewardsAdministrator = 25,

}
