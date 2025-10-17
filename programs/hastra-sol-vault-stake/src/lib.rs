pub mod account_structs;
/// # hastra sol vault stake - Token Staking System
///
/// ## Business Process Flow
///
/// 1. Initial Setup:
///    - Admin creates two token types: Vault (wYLDS), Stake (PRIME)
///    - Admin initializes program with token addresses and unbonding period
///    - Admin configures vault token account to hold deposited tokens
///
/// 2. User Staking Flow:
///    a. Deposit Phase:
///       - User deposits vault tokens (wYLDS)
///       - System securely stores tokens in vault account
///       - User receives equivalent stake tokens (PRIME)
///
/// 3. Withdrawal Flow:
///    a. Unbonding Initiation:
///       - User initiates withdrawal by burning stake tokens (PRIME)
///       - System creates an unbonding ticket attached to the user
///       - Unbonding period timer starts
///    
///    b. Waiting Period:
///       - User holds an unbonding ticket during lock period
///       - Can query remaining time via status check
///    
///    c. Redemption:
///       - After the unbonding period expires, the user can redeem
///       - Original vault tokens (wYLDS) returned to user
///      - Unbonding ticket is invalidated
///
/// 4. Administrative Functions:
///    - Update token configurations if needed
///    - Manage mint authorities
///    - Monitor vault token accounts
///
/// Security is maintained through PDAs (Program Derived Addresses) and strict
/// token authority controls. All token operations are atomic and validated
/// through Solana's transaction model.
pub mod error;
pub mod events;
mod guard;
pub mod processor;
pub mod state;

use account_structs::*;
use anchor_lang::prelude::*;
use state::ProofNode;

declare_id!("dyXhxx6Y6LeMwZwb78oeTGWqwJkufPAMFEzH2QJ4mcp");

#[program]
pub mod hastra_sol_vault_stake {
    use super::*;

    /// Initializes the vault program with the required token configurations:
    /// - vault_mint: The token that users deposit (e.g., wYLDS)
    /// - stake_mint: The token users receive when staking (e.g., PRIME)
    /// - unbonding_period: Time in seconds users must wait before redeeming
    pub fn initialize(
        ctx: Context<Initialize>,
        vault_mint: Pubkey,
        stake_mint: Pubkey,
        unbonding_period: i64,
        freeze_administrators: Vec<Pubkey>,
        rewards_administrators: Vec<Pubkey>,
    ) -> Result<()> {
        processor::initialize(
            ctx,
            vault_mint,
            stake_mint,
            unbonding_period,
            freeze_administrators,
            rewards_administrators,
        )
    }

    /// Pauses or unpauses the protocol operations:
    /// - pause: true to pause, false to unpause
    pub fn pause(ctx: Context<Pause>, pause: bool) -> Result<()> {
        processor::pause(ctx, pause)
    }
    
    /// Updates the program configuration with new token addresses:
    /// - new_unbonding_period: New unbonding period in seconds
    pub fn update_config(ctx: Context<UpdateConfig>, new_unbonding_period: i64) -> Result<()> {
        processor::update_config(ctx, new_unbonding_period)
    }

    /// Handles user deposits of vault tokens (e.g., wYLDS):
    /// - Transfers vault tokens to program vault account
    /// - Mints equivalent amount of stake tokens (e.g., PRIME) to user
    pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
        processor::deposit(ctx, amount)
    }

    /// Initiates the unbonding process:
    /// - Burns user's stake tokens (e.g., PRIME)
    /// - Starts unbonding period timer via user ticket
    pub fn unbond(ctx: Context<Unbond>, amount: u64) -> Result<()> {
        processor::unbond(ctx, amount)
    }

    /// Completes the unbonding process after the period expires:
    /// - Burns unbonding tokens (e.g., uwYLDS)
    /// - Returns vault tokens (e.g., wYLDS) to user
    pub fn redeem(ctx: Context<Redeem>) -> Result<()> {
        processor::redeem(ctx)
    }

    pub fn update_freeze_administrators(
        ctx: Context<UpdateFreezeAdministrators>,
        new_administrators: Vec<Pubkey>,
    ) -> Result<()> {
        processor::update_freeze_administrators(ctx, new_administrators)
    }

    pub fn freeze_token_account(ctx: Context<FreezeTokenAccount>) -> Result<()> {
        processor::freeze_token_account(ctx)
    }
    pub fn thaw_token_account(ctx: Context<ThawTokenAccount>) -> Result<()> {
        processor::thaw_token_account(ctx)
    }

    pub fn update_rewards_administrators(
        ctx: Context<UpdateRewardsAdministrators>,
        new_administrators: Vec<Pubkey>,
    ) -> Result<()> {
        processor::update_rewards_administrators(ctx, new_administrators)
    }

    pub fn create_rewards_epoch(
        ctx: Context<CreateRewardsEpoch>,
        index: u64,
        merkle_root: [u8; 32],
        total: u64,
    ) -> Result<()> {
        processor::create_rewards_epoch(ctx, index, merkle_root, total)
    }

    /// This is the classic “airdrop/claim per epoch” design
    /// High-level idea:
    /// 	1.	Off-chain (admin does this each epoch):
    /// 	•	Calculate each user’s reward for this epoch.
    /// 	•	Build a Merkle tree of (user, amount, epoch_index).
    /// 	•	Publish the Merkle root on-chain with create_rewards_epoch function above.
    ///
    /// 	2.	On-chain:
    /// 	•	Store each epoch’s Merkle root in a PDA.
    /// 	•	When a user claims, they present (amount, proof) for their pubkey.
    /// 	•	The program verifies the Merkle proof against the root.
    /// 	•	If valid, transfer reward tokens (PRIME) from the rewards vault to the user's staking mint token account.
    /// 	•	Mark the claim as redeemed so they can’t double-claim.
    pub fn claim_rewards(
        ctx: Context<ClaimRewards>,
        amount: u64,
        proof: Vec<ProofNode>,
    ) -> Result<()> {
        processor::claim_rewards(ctx, amount, proof)
    }
}
