use crate::account_structs::*;
use crate::error::*;
use crate::guard::validate_program_update_authority;
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
    require!(
        freeze_administrators.len() <= 5,
        CustomErrorCode::TooManyAdministrators
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

pub fn update_config(ctx: Context<UpdateConfig>, new_unbonding_period: i64) -> Result<()> {
    validate_program_update_authority(&ctx.accounts.program_data, &ctx.accounts.signer)?;
    let config = &mut ctx.accounts.config;
    config.unbonding_period = new_unbonding_period;
    Ok(())
}

pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
    require!(amount > 0, CustomErrorCode::InvalidAmount);

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
    Ok(())
}

pub fn unbond(ctx: Context<Unbond>, amount: u64) -> Result<()> {
    require!(amount > 0, CustomErrorCode::InvalidAmount);

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

    Ok(())
}

pub fn redeem(ctx: Context<Redeem>) -> Result<()> {
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

    Ok(())
}

pub fn set_mint_authority(ctx: Context<SetMintAuthority>, new_authority: Pubkey) -> Result<()> {
    // Validate that the signer is the program's update authority
    validate_program_update_authority(&ctx.accounts.program_data, &ctx.accounts.signer)?;

    let mint = &ctx.accounts.mint;
    let token_program = &ctx.accounts.token_program;

    // Create seeds for PDA signing
    let mint_authority_signer: &[&[&[u8]]] =
        &[&["mint_authority".as_bytes(), &[ctx.bumps.mint_authority]]];

    let cpi_accounts = token::SetAuthority {
        account_or_mint: mint.to_account_info(),
        current_authority: ctx.accounts.mint_authority.to_account_info(),
    };

    let cpi_ctx = CpiContext::new_with_signer(
        token_program.to_account_info(),
        cpi_accounts,
        mint_authority_signer,
    );

    token::set_authority(cpi_ctx, AuthorityType::MintTokens, Some(new_authority))?;

    msg!(
        "Mint authority changed from {} to {}",
        ctx.accounts.mint_authority.key(),
        new_authority
    );
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
        new_administrators.len() <= 5,
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
        new_administrators.len() <= 5,
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

pub fn create_rewards_epoch(
    ctx: Context<CreateRewardsEpoch>,
    index: u64,
    merkle_root: [u8; 32],
    total: u64,
) -> Result<()> {
    require!(
        ctx.accounts
            .config
            .rewards_administrators
            .contains(&ctx.accounts.admin.key()),
        CustomErrorCode::InvalidRewardsAdministrator
    );
    let e = &mut ctx.accounts.epoch;
    e.index = index;
    e.merkle_root = merkle_root;
    e.total = total;
    e.created_ts = Clock::get()?.unix_timestamp;
    Ok(())
}

pub fn claim_rewards(ctx: Context<ClaimRewards>, amount: u64, proof: Vec<[u8; 32]>) -> Result<()> {
    require!(amount > 0, CustomErrorCode::InvalidAmount);
    // leaf = sha256(user || amount_le || epoch_index_le)
    let mut data = Vec::with_capacity(32 + 8 + 8);
    data.extend_from_slice(ctx.accounts.user.key.as_ref());
    data.extend_from_slice(&amount.to_le_bytes());
    data.extend_from_slice(&ctx.accounts.epoch.index.to_le_bytes());
    let mut node = hashv(&[&data]).to_bytes();

    // Merkle verify (sorted pairs)
    for sib in &proof {
        let (a, b) = if node <= *sib {
            (node, *sib)
        } else {
            (*sib, node)
        };
        node = hashv(&[&a, &b]).to_bytes();
    }
    require!(
        node == ctx.accounts.epoch.merkle_root,
        CustomErrorCode::InvalidMerkleProof
    );

    // mint staking tokens (sYLDS) to user
    let seeds: &[&[u8]] = &[b"mint_authority", &[ctx.bumps.mint_authority]];
    let signer = &[&seeds[..]];
    let cpi_accounts = MintTo {
        mint: ctx.accounts.mint.to_account_info(),
        to: ctx.accounts.user_stake_token_account.to_account_info(),
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
    Ok(())
}
