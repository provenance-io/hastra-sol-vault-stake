use crate::account_structs::*;
use crate::error::*;
use crate::events::*;
use crate::guard::validate_program_update_authority;
use crate::state::{ProofNode, MAX_ADMINISTRATORS, MAX_UNBONDING_PERIOD, MIN_UNBONDING_PERIOD};
use anchor_lang::prelude::*;
use anchor_lang::solana_program::hash::hashv;
use anchor_spl::token::spl_token::instruction::AuthorityType;
use anchor_spl::token::{self, Burn, MintTo, Transfer};

pub fn initialize(
    ctx: Context<Initialize>,
    vault_mint: Pubkey,
    stake_mint: Pubkey,
    unbonding_period: i64,
    freeze_administrators: Vec<Pubkey>,
    rewards_administrators: Vec<Pubkey>,
) -> Result<()> {
    validate_program_update_authority(&ctx.accounts.program_data, &ctx.accounts.signer)?;
    require!(
        freeze_administrators.len() <= MAX_ADMINISTRATORS,
        CustomErrorCode::TooManyAdministrators
    );
    require!(
        rewards_administrators.len() <= MAX_ADMINISTRATORS,
        CustomErrorCode::TooManyAdministrators
    );
    require!(unbonding_period >= MIN_UNBONDING_PERIOD, CustomErrorCode::InvalidBondingPeriod);
    require!(
        unbonding_period <= MAX_UNBONDING_PERIOD,
        CustomErrorCode::InvalidBondingPeriod
    ); 
    require!(
        vault_mint != stake_mint,
        CustomErrorCode::VaultAndMintCannotBeSame
    );

    let config = &mut ctx.accounts.config;
    config.vault = vault_mint;
    config.mint = stake_mint;
    config.unbonding_period = unbonding_period;
    config.freeze_administrators = freeze_administrators;
    config.rewards_administrators = rewards_administrators;
    config.bump = ctx.bumps.config;

    // The vault token account must be owned by the program-derived address (PDA)
    // and is the token account that holds the deposited vault tokens (e.g., wYLDS).
    // This ensures that only the program can move tokens out of this account.
    // Only set vault token account to PDA authority if it's not already set to vault_authority
    if ctx.accounts.vault_token_account.owner == ctx.accounts.signer.key() {
        let seeds: &[&[u8]] = &[b"vault_authority", &[ctx.bumps.vault_authority]];
        let signer = &[&seeds[..]];
        token::set_authority(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                token::SetAuthority {
                    account_or_mint: ctx.accounts.vault_token_account.to_account_info(),
                    current_authority: ctx.accounts.signer.to_account_info(),
                },
                signer,
            ),
            AuthorityType::AccountOwner,
            Some(ctx.accounts.vault_authority.key()),
        )?;
    }
    Ok(())
}

pub fn pause(ctx: Context<Pause>, pause: bool) -> Result<()> {
    validate_program_update_authority(&ctx.accounts.program_data, &ctx.accounts.signer)?;
    let config = &mut ctx.accounts.config;
    config.paused = pause;

    msg!("Protocol paused: {}", pause);

    Ok(())
}

pub fn update_config(ctx: Context<UpdateConfig>, new_unbonding_period: i64) -> Result<()> {
    validate_program_update_authority(&ctx.accounts.program_data, &ctx.accounts.signer)?;
    require!(new_unbonding_period >= MIN_UNBONDING_PERIOD, CustomErrorCode::InvalidBondingPeriod);
    require!(
        new_unbonding_period <= MAX_UNBONDING_PERIOD,
        CustomErrorCode::InvalidBondingPeriod
    );

    let config = &mut ctx.accounts.config;
    config.unbonding_period = new_unbonding_period;

    emit!(UnbondingPeriodUpdated {
        admin: ctx.accounts.signer.key(),
        old_period: ctx.accounts.config.unbonding_period,
        new_period: new_unbonding_period,
        mint: ctx.accounts.config.mint,
        vault: ctx.accounts.config.vault,
    });

    Ok(())
}

pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
    require!(amount > 0, CustomErrorCode::InvalidAmount);
    require!(!ctx.accounts.config.paused, CustomErrorCode::ProtocolPaused);

    let cpi_accounts = Transfer {
        from: ctx.accounts.user_vault_token_account.to_account_info(),
        to: ctx.accounts.vault_token_account.to_account_info(),
        authority: ctx.accounts.signer.to_account_info(),
    };
    token::transfer(
        CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_accounts),
        amount,
    )?;

    let seeds: &[&[u8]] = &[b"mint_authority", &[ctx.bumps.mint_authority]];
    let signer = &[&seeds[..]];
    let cpi_accounts = MintTo {
        mint: ctx.accounts.mint.to_account_info(),
        to: ctx.accounts.user_mint_token_account.to_account_info(),
        authority: ctx.accounts.mint_authority.to_account_info(),
    };
    token::mint_to(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
            signer,
        ),
        amount,
    )?;

    msg!("Emitting DepositEvent");
    emit!(DepositEvent {
        user: ctx.accounts.signer.key(),
        amount,
        mint: ctx.accounts.mint.key(),
        vault: ctx.accounts.vault_token_account.key(),
    });
    msg!("Emitted DepositEvent");

    Ok(())
}

pub fn unbond(ctx: Context<Unbond>, amount: u64) -> Result<()> {
    require!(amount > 0, CustomErrorCode::InvalidAmount);
    require!(!ctx.accounts.config.paused, CustomErrorCode::ProtocolPaused);

    let current_mint_amount = ctx.accounts.user_mint_token_account.amount;
    require!(
        amount <= current_mint_amount,
        CustomErrorCode::InsufficientUnbondingBalance
    );

    let ticket = &mut ctx.accounts.ticket;
    ticket.owner = ctx.accounts.signer.key();
    ticket.requested_amount = amount;
    ticket.start_balance = current_mint_amount;
    ticket.start_ts = Clock::get()?.unix_timestamp;

    msg!("Emitting UnbondEvent");
    emit!(UnbondEvent {
        user: ctx.accounts.signer.key(),
        amount,
        mint: ctx.accounts.mint.key(),
        vault: ctx.accounts.config.vault,
    });
    msg!("Emitted UnbondingEvent");

    Ok(())
}

pub fn redeem(ctx: Context<Redeem>) -> Result<()> {
    require!(!ctx.accounts.config.paused, CustomErrorCode::ProtocolPaused);
    let now = Clock::get()?.unix_timestamp;
    let ticket = &ctx.accounts.ticket;
    require_keys_eq!(
        ticket.owner,
        ctx.accounts.signer.key(),
        CustomErrorCode::InvalidTicketOwner
    );

    let config = &ctx.accounts.config;

    require!(
        now - ticket.start_ts >= config.unbonding_period,
        CustomErrorCode::UnbondingPeriodNotElapsed
    );

    let current_mint_amount = ctx.accounts.user_mint_token_account.amount;
    let redeem = ticket.requested_amount.min(current_mint_amount);
    require!(redeem > 0, CustomErrorCode::InsufficientUnbondingBalance);

    require!(
        ctx.accounts.vault_token_account.amount >= redeem,
        CustomErrorCode::InsufficientVaultBalance
    );

    let burn_accounts = Burn {
        mint: ctx.accounts.mint.to_account_info(),
        from: ctx.accounts.user_mint_token_account.to_account_info(),
        authority: ctx.accounts.signer.to_account_info(),
    };
    token::burn(
        CpiContext::new(ctx.accounts.token_program.to_account_info(), burn_accounts),
        redeem,
    )?;

    let seeds: &[&[u8]] = &[b"vault_authority", &[ctx.bumps.vault_authority]];
    let signer = &[&seeds[..]];
    let transfer_accounts = Transfer {
        from: ctx.accounts.vault_token_account.to_account_info(),
        to: ctx.accounts.user_vault_token_account.to_account_info(),
        authority: ctx.accounts.vault_authority.to_account_info(),
    };
    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            transfer_accounts,
            signer,
        ),
        redeem,
    )?;

    msg!("Emitting RedeemEvent");
    emit!(RedeemEvent {
        user: ctx.accounts.signer.key(),
        amount: redeem,
        mint: ctx.accounts.mint.key(),
        vault: ctx.accounts.vault_token_account.key(),
    });
    msg!("Emitted RedeemEvent");
    
    Ok(())
}

// Set the mint token's freeze authority to the program PDA
// Update the list of freeze administrators (only program update authority can do this)
pub fn update_freeze_administrators(
    ctx: Context<UpdateFreezeAdministrators>,
    new_administrators: Vec<Pubkey>,
) -> Result<()> {
    // Validate that the signer is the program's update authority
    validate_program_update_authority(&ctx.accounts.program_data, &ctx.accounts.signer)?;

    let config = &mut ctx.accounts.config;

    require!(
        new_administrators.len() <= MAX_ADMINISTRATORS,
        CustomErrorCode::TooManyAdministrators
    );

    config.freeze_administrators = new_administrators;

    msg!(
        "Freeze administrators updated. New count: {}",
        config.freeze_administrators.len()
    );
    Ok(())
}

// Set the mint token's rewards authority to the program PDA
// Update the list of rewards administrators (only program update authority can do this)
pub fn update_rewards_administrators(
    ctx: Context<UpdateRewardsAdministrators>,
    new_administrators: Vec<Pubkey>,
) -> Result<()> {
    // Validate that the signer is the program's update authority
    validate_program_update_authority(&ctx.accounts.program_data, &ctx.accounts.signer)?;

    let config = &mut ctx.accounts.config;

    require!(
        new_administrators.len() <= MAX_ADMINISTRATORS,
        CustomErrorCode::TooManyAdministrators
    );

    config.rewards_administrators = new_administrators;

    msg!(
        "Rewards administrators updated. New count: {}",
        config.freeze_administrators.len()
    );
    Ok(())
}

// Freeze a specific token account (only freeze administrators can do this)
pub fn freeze_token_account(ctx: Context<FreezeTokenAccount>) -> Result<()> {
    let config = &ctx.accounts.config;
    let signer = ctx.accounts.signer.key();

    // Verify signer is a freeze administrator
    require!(
        config.freeze_administrators.contains(&signer),
        CustomErrorCode::UnauthorizedFreezeAdministrator
    );

    let freeze_authority_seeds: &[&[&[u8]]] =
        &[&[b"freeze_authority", &[ctx.bumps.freeze_authority_pda]]];

    let cpi_accounts = token::FreezeAccount {
        account: ctx.accounts.token_account.to_account_info(),
        mint: ctx.accounts.mint.to_account_info(),
        authority: ctx.accounts.freeze_authority_pda.to_account_info(),
    };

    let cpi_ctx = CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        cpi_accounts,
        freeze_authority_seeds,
    );

    token::freeze_account(cpi_ctx)?;

    msg!(
        "Token account {} frozen by administrator {}",
        ctx.accounts.token_account.key(),
        signer
    );
    Ok(())
}

// Thaw a specific token account (only freeze administrators can do this)
pub fn thaw_token_account(ctx: Context<ThawTokenAccount>) -> Result<()> {
    let config = &ctx.accounts.config;
    let signer = ctx.accounts.signer.key();

    // Verify signer is a freeze administrator
    require!(
        config.freeze_administrators.contains(&signer),
        CustomErrorCode::UnauthorizedFreezeAdministrator
    );

    let freeze_authority_seeds: &[&[&[u8]]] =
        &[&[b"freeze_authority", &[ctx.bumps.freeze_authority_pda]]];

    let cpi_accounts = token::ThawAccount {
        account: ctx.accounts.token_account.to_account_info(),
        mint: ctx.accounts.mint.to_account_info(),
        authority: ctx.accounts.freeze_authority_pda.to_account_info(),
    };

    let cpi_ctx = CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        cpi_accounts,
        freeze_authority_seeds,
    );

    token::thaw_account(cpi_ctx)?;

    msg!(
        "Token account {} thawed by administrator {}",
        ctx.accounts.token_account.key(),
        signer
    );
    Ok(())
}

pub fn publish_rewards(
    ctx: Context<PublishRewards>,
    amount: u64,
) -> Result<()> {
    require!(
        ctx.accounts
            .config
            .rewards_administrators
            .contains(&ctx.accounts.admin.key()),
        CustomErrorCode::InvalidRewardsAdministrator
    );
    require!(amount > 0, CustomErrorCode::InvalidAmount);

    // at this point, use CPI to call the mint_to instruction on the hastra-sol-vault-mint 
    
    msg!("Emitting RewardsPublished");
    emit!(RewardsPublished {
        admin: ctx.accounts.admin.key(),
        amount: amount,
        mint_program: ctx.accounts.mint_program.key(),
        vault_token_account: ctx.accounts.vault_token_account.key(),
        mint: ctx.accounts.config.mint,
        vault: ctx.accounts.config.vault,
    });
    msg!("Emitted RewardsPublished");

    Ok(())
}
