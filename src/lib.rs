pub fn f(){}
use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    bpf_loader_upgradeable,
    hash::hash,
    program::invoke_signed,
};

declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

pub const MAX_SIGNERS: usize = 10;
pub const MAX_ACTION_SIZE: usize = 400;
pub const PROPOSAL_STATUS_ACTIVE: u8 = 0;
pub const PROPOSAL_STATUS_EXECUTED: u8 = 1;

#[program]
pub mod multisig {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, owners: Vec<Pubkey>, threshold: u16) -> Result<()> {
        require!(!owners.is_empty(), ErrorCode::InvalidOwners);
        require!(threshold > 0 && (threshold as usize) <= owners.len(), ErrorCode::InvalidThreshold);
        let multisig = &mut ctx.accounts.multisig;
        multisig.owners = owners.clone();
        multisig.threshold = threshold;
        multisig.proposal_count = 0;
        multisig.paused = false;
        multisig.authority_set_hash = hash_owners(&owners);
        multisig.bump = ctx.bumps.multisig;
        Ok(())
    }

    pub fn create_proposal(ctx: Context<CreateProposal>, action_data: Vec<u8>) -> Result<()> {
        let multisig = &mut ctx.accounts.multisig;
        require!(multisig.owners.contains(&ctx.accounts.proposer.key()), ErrorCode::NotSigner);
        let proposal = &mut ctx.accounts.proposal;
        proposal.multisig = multisig.key();
        proposal.proposer = ctx.accounts.proposer.key();
        proposal.action_data = action_data;
        proposal.signers = multisig.owners.clone();
        proposal.approvals = vec![];
        proposal.threshold = multisig.threshold;
        proposal.status = PROPOSAL_STATUS_ACTIVE;
        proposal.authority_set_hash = multisig.authority_set_hash;
        proposal.created_at = Clock::get()?.unix_timestamp;
        proposal.bump = ctx.bumps.proposal;
        multisig.proposal_count += 1;
        Ok(())
    }

    pub fn approve(ctx: Context<Approve>) -> Result<()> {
        let multisig = &ctx.accounts.multisig;
        let proposal = &mut ctx.accounts.proposal;
        require!(proposal.multisig == multisig.key(), ErrorCode::InvalidMultisig);
        require!(proposal.status == PROPOSAL_STATUS_ACTIVE, ErrorCode::ProposalNotActive);
        require!(proposal.authority_set_hash == multisig.authority_set_hash, ErrorCode::StaleProposal);
        require!(proposal.signers.contains(ctx.accounts.approver.key), ErrorCode::NotSigner);
        require!(!proposal.approvals.contains(ctx.accounts.approver.key), ErrorCode::AlreadyApproved);
        proposal.approvals.push(ctx.accounts.approver.key());
        Ok(())
    }

    pub fn execute(ctx: Context<Execute>) -> Result<()> {
        let multisig = &mut ctx.accounts.multisig;
        let proposal = &mut ctx.accounts.proposal;

        require!(proposal.multisig == multisig.key(), ErrorCode::InvalidMultisig);
        require!(proposal.status == PROPOSAL_STATUS_ACTIVE, ErrorCode::ProposalNotActive);
        require!(proposal.authority_set_hash == multisig.authority_set_hash, ErrorCode::StaleProposal);
        require!(proposal.approvals.len() as u16 >= proposal.threshold, ErrorCode::ThresholdNotMet);
        for signer in &proposal.approvals {
            require!(proposal.signers.contains(signer), ErrorCode::InvalidApproval);
        }

        let action = Action::try_from_slice(&proposal.action_data)
            .map_err(|_| ErrorCode::InvalidAction)?;

        match action {
            Action::Pause => {
                multisig.paused = true;
            }
            Action::Unpause => {
                multisig.paused = false;
            }
            Action::ChangeThreshold(new_threshold) => {
                require!(new_threshold > 0 && (new_threshold as usize) <= multisig.owners.len(), ErrorCode::InvalidThreshold);
                multisig.threshold = new_threshold;
            }
            Action::ChangeOwners(new_owners) => {
                require!(!new_owners.is_empty(), ErrorCode::InvalidOwners);
                multisig.owners = new_owners;
                multisig.authority_set_hash = hash_owners(&multisig.owners);
            }
            Action::Sweep { amount, destination } => {
                sweep(ctx.remaining_accounts, amount, destination)?;
            }
            Action::Upgrade { program, buffer, spill } => {
                upgrade_program(ctx.remaining_accounts, multisig.to_account_info(), program, buffer, spill, multisig.bump)?;
            }
        }

        proposal.status = PROPOSAL_STATUS_EXECUTED;
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = authority, space = Multisig::SPACE, seeds = [b"multisig"], bump)]
    pub multisig: Account<'info, Multisig>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CreateProposal<'info> {
    #[account(mut, seeds = [b"multisig"], bump)]
    pub multisig: Account<'info, Multisig>,
    #[account(
        init,
        payer = proposer,
        space = Proposal::SPACE,
        seeds = [b"proposal", multisig.key().as_ref(), &multisig.proposal_count.to_le_bytes()],
        bump
    )]
    pub proposal: Account<'info, Proposal>,
    #[account(mut)]
    pub proposer: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Approve<'info> {
    #[account(seeds = [b"multisig"], bump)]
    pub multisig: Account<'info, Multisig>,
    #[account(mut)]
    pub proposal: Account<'info, Proposal>,
    pub approver: Signer<'info>,
}

#[derive(Accounts)]
pub struct Execute<'info> {
    #[account(mut, seeds = [b"multisig"], bump)]
    pub multisig: Account<'info, Multisig>,
    #[account(mut)]
    pub proposal: Account<'info, Proposal>,
}

#[account]
#[derive(Default)]
pub struct Multisig {
    pub owners: Vec<Pubkey>,
    pub threshold: u16,
    pub proposal_count: u64,
    pub paused: bool,
    pub authority_set_hash: [u8; 32],
    pub bump: u8,
}

#[account]
#[derive(Default)]
pub struct Proposal {
    pub multisig: Pubkey,
    pub proposer: Pubkey,
    pub signers: Vec<Pubkey>,
    pub approvals: Vec<Pubkey>,
    pub threshold: u16,
    pub action_data: Vec<u8>,
    pub status: u8,
    pub authority_set_hash: [u8; 32],
    pub created_at: i64,
    pub bump: u8,
}

#[derive(Clone, AnchorSerialize, AnchorDeserialize)]
pub enum Action {
    Pause,
    Unpause,
    ChangeThreshold(u16),
    ChangeOwners(Vec<Pubkey>),
    Sweep { amount: u64, destination: Pubkey },
    Upgrade { program: Pubkey, buffer: Pubkey, spill: Pubkey },
}

fn hash_owners(owners: &[Pubkey]) -> [u8; 32] {
    let mut sorted = owners.to_vec();
    sorted.sort();
    let mut buf = Vec::new();
    for o in sorted {
        buf.extend_from_slice(o.as_ref());
    }
    hash(&buf).to_bytes()
}

fn sweep(remaining_accounts: &[AccountInfo], amount: u64, destination: Pubkey) -> Result<()> {
    let vault_info = remaining_accounts.first().ok_or(ErrorCode::InvalidSweepAccounts)?;
    let dest_info = remaining_accounts.get(1).ok_or(ErrorCode::InvalidSweepAccounts)?;
    let (expected_vault, _) = Pubkey::find_program_address(&[b"vault"], &ID);
    require!(vault_info.key == &expected_vault, ErrorCode::InvalidVault);
    require!(&destination == dest_info.key, ErrorCode::InvalidDestination);
    require!(vault_info.lamports() >= amount, ErrorCode::InsufficientFunds);

    **vault_info.try_borrow_mut_lamports()? -= amount;
    **dest_info.try_borrow_mut_lamports()? += amount;
    Ok(())
}

fn upgrade_program(
    remaining_accounts: &[AccountInfo],
    multisig_info: AccountInfo,
    program: Pubkey,
    buffer: Pubkey,
    spill: Pubkey,
    multisig_bump: u8,
) -> Result<()> {
    let program_info = remaining_accounts.first().ok_or(ErrorCode::InvalidUpgradeAccounts)?;
    let buffer_info = remaining_accounts.get(1).ok_or(ErrorCode::InvalidUpgradeAccounts)?;
    let spill_info = remaining_accounts.get(2).ok_or(ErrorCode::InvalidUpgradeAccounts)?;
    require!(program_info.key == &program, ErrorCode::InvalidUpgradeAccounts);
    require!(buffer_info.key == &buffer, ErrorCode::InvalidUpgradeAccounts);
    require!(spill_info.key == &spill, ErrorCode::InvalidUpgradeAccounts);

    let authority_info = multisig_info;
    let upgrade_ix = bpf_loader_upgradeable::upgrade(
        &program,
        &buffer,
        authority_info.key,
        &spill,
    );
    let account_infos = vec![
        program_info.clone(),
        buffer_info.clone(),
        authority_info,
        spill_info.clone(),
    ];
    invoke_signed(
        &upgrade_ix,
        &account_infos,
        &[&[b"multisig", &[multisig_bump]]],
    )?;
    Ok(())
}

impl Multisig {
    pub const SPACE: usize = 8 + 4 + MAX_SIGNERS * 32 + 2 + 8 + 1 + 32 + 1;
}

impl Proposal {
    pub const SPACE: usize = 8 + 32 + 32 + (4 + MAX_SIGNERS * 32) + (4 + MAX_SIGNERS * 32) + 2 + (4 + MAX_ACTION_SIZE) + 1 + 32 + 8 + 1;
}

#[error_code]
pub enum ErrorCode {
    #[msg("Only signers can perform this action")]
    NotSigner,
    #[msg("Threshold must be between 1 and number of owners")]
    InvalidThreshold,
    #[msg("Proposal is not active")]
    ProposalNotActive,
    #[msg("Proposal is stale")]
    StaleProposal,
    #[msg("Approver has already approved")]
    AlreadyApproved,
    #[msg("Threshold not met")]
    ThresholdNotMet,
    #[msg("Invalid approval")]
    InvalidApproval,
    #[msg("Invalid action data")]
    InvalidAction,
    #[msg("Invalid vault")]
    InvalidVault,
    #[msg("Invalid destination")]
    InvalidDestination,
    #[msg("Insufficient funds")]
    InsufficientFunds,
    #[msg("Invalid sweep accounts")]
    InvalidSweepAccounts,
    #[msg("Invalid upgrade accounts")]
    InvalidUpgradeAccounts,
    #[msg("Multisig mismatch")]
    InvalidMultisig,
    #[msg("Owners cannot be empty")]
    InvalidOwners,
}
