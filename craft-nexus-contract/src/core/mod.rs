//! Core domain modules for the Craft Nexus contract.
///
/// This module groups the business logic of the contract into
/// domain-specific sub-modules. Each sub-module exposes free functions
/// that operate on the contract environment and are routed to from the
/// thin `lib.rs` entrypoint layer.

pub use crate::types;
pub use crate::error;
pub use crate::storage;

pub mod admin;
pub mod escrow;
pub mod reconciliation;
pub mod recurring;
pub mod staking;
