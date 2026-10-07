use soroban_std::{contracterror, ContractError};
use core::fmt;

/// Error types for the Craft Nexus contract.
///
/// This module houses the contract's top-level error enum and the
/// conversions required by the Soroban runtime. It was extracted from
/// `lib.rs` as part of the modularization RFC (Phine 1).

#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrder)]
#[contracterror]
pub enum Error {
    /// The contract has not been initialized yet.
    NotInitialized = 1,
    /// The contract has already been initialized.
    AlreadyInitialized = 2,
    /// The caller is not authorized to perform this action.
    Unauthorized = 3,
    /// The contract is paused and cannot process this operation.
    ContractPaused = 4,
    /// The requested escrow was not found.
    EscrowNotFound = 5,
    /// The escrow is not in a valid state for this operation.
    InvalidEscrowState = 6,
    /// The provided amount is invalid (negative or zero where not allowed).
    InvalidAmount = 7,
    /// The provided deadline is invalid.
    InvalidDeadline = 8,
    /// The caller is not a party to the escrow.
    NotEscrowParty = 9,
    /// The escrow has already been released or refunded.
    EscrowCompleted = 10,
    /// The escrow has not yet expired.
    EscrowNotExpired = 11,
    /// The artisan is not staked.
    NotStaked = 12,
    /// The artisan already has an active stake.
    AlreadyStaked = 13,
    /// The stake amount is below the minimum required.
    InsufficientStake = 14,
    /// The artisan has been slashed and cannot participate.
    ArtisanSlashed = 15,
    /// The provided repair action is not recognized.
    InvalidRepairAction = 16,
    /// The reconciliation report was not found.
    ReportNotFound = 17,
    /// The reconciliation report has already been approved.
    ReportAlreadyApproved = 18,
    /// The reconciliation report has already been resolved.
    ReportAlreadyResolved = 19,
    /// The provided argument is invalid.
    InvalidArgument = 20,
    /// The contract has not been initialized with a token address.
    TokenNotSet = 21,
    /// The contract has already been initialized with a token address.
    TokenAlreadySet = 22,
    /// The token transfer failed.
    TokenTransferFailed = 23,
    /// The provided fee is invalid.
    InvalidFee = 24,
    /// The provided address is invalid.
    InvalidAddress = 25,
    /// The provided string is too long.
    StringTooLong = 26,
    /// The provided string is empty.
    EmptyString = 27,
    /// The requested recurring escrow was not found.
    RecurringNotFound = 28,
    /// The recurring escrow is not in a valid state for this operation.
    InvalidRecurringState = 29,
    /// The caller is not a party to the recurring escrow.
    NotRecurringParty = 30,
    /// The recurring escrow has already been cancelled.
    RecurringCancelled = 31,
    /// The recurring escrow has already been completed.
    RecurringCompleted = 32,
    /// The admin address has not been set.
    AdminNotSet = 33,
    /// The admin address has already been set.
    AdminAlreadySet = 34,
    /// The caller is not the admin.
    NotAdmin = 35,
    /// The contract is already paused.
    AlreadyPaused = 36,
    /// The contract is not paused.
    NotPaused = 37,
    /// The provided fee percentage is out of bounds.
    FeePercentageOutOfRange = 38,
    /// The provided threshold is out of bounds.
    ThresholdOutOfRange = 39,
    /// The operation would overflow an arithmetic calculation.
    ArithmeticOverflow = 40,
    /// The operation is not supported in the current context.
    NotSupported = 41,
    /// A generic internal error occurred.
    InternalError = 42,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let msg = match self {
            Error::NotInitialized => "contract not initialized",
            Error::AlreadyInitialized => "contract already initialized",
            Error::Unauthorized => "unauthorized",
            Error::ContractPaused => "contract is paused",
            Error::EscrowNotFound => "escrow not found",
            Error::InvalidEscrowState => "invalid escrow state",
            Error::InvalidAmount => "invalid amount",
            Error::InvalidDeadline => "invalid deadline",
            Error::NotEscrowParty => "not an escrow party",
            Error::EscrowCompleted => "escrow already completed",
            Error::EscrowNotExpired => "escrow has not expired",
            Error::NotStaked => "artisan not staked",
            Error::AlreadyStaked => "artisan already staked",
            Error::InsufficientStake => "insufficient stake",
            Error::ArtisanSlashed => "artisan has been slashed",
            Error::InvalidRepairAction => "invalid repair action",
            Error::ReportNotFound => "reconciliation report not found",
            Error::ReportAlreadyApproved => "report already approved",
            Error::ReportAlreadyResolved => "report already resolved",
            Error::InvalidArgument => "invalid argument",
            Error::TokenNotSet => "token not set",
            Error::TokenAlreadySet => "token already set",
            Error::TokenTransferFailed => "token transfer failed",
            Error::InvalidFee => "invalid fee",
            Error::InvalidAddress => "invalid address",
            Error::StringTooLong => "string too long",
            Error::EmptyString => "empty string",
            Error::RecurringNotFound => "recurring escrow not found",
            Error::InvalidRecurringState => "invalid recurring escrow state",
            Error::NotRecurringParty => "not a recurring escrow party",
            Error::RecurringCancelled => "recurring escrow cancelled",
            Error::RecurringCompleted => "recurring escrow completed",
            Error::AdminNotSet => "admin not set",
            Error::AdminAlreadySet => "admin already set",
            Error::NotAdmin => "not admin",
            Error::AlreadyPaused => "contract already paused",
            Error::NotPaused => "contract not paused",
            Error::FeePercentageOutOfRange => "fee percentage out of range",
            Error::ThresholdOutOfRange => "threshold out of range",
            Error::ArithmeticOverflow => "arithmetic overflow",
            Error::NotSupported => "operation not supported",
            Error::InternalError => "internal error",
        };
        write!(f, "{}", msg)
    }
}

impl From<Error> for ContractError {
    fn from(err: Error) -> ContractError {
        ContractError::from_contract_error(err as u)
    }
}
