#![no_std]

use soroban_sdk::{contract, contracterror, contracttype, contractimpl, Address, Env};

/// The pooled donor funds for a single reforestation project.
///
/// Donors deposit into the pool named by `project_id`; there is no
/// per-donor accrual rate to track like a payment stream — instead, a
/// fraction of `total_deposited` releases to `recipient` each time the
/// `attestor` confirms a milestone, per the project's milestone schedule.
#[contracttype]
#[derive(Clone)]
pub struct ProjectVault {
    pub recipient: Address,
    pub attestor: Address,
    pub token: Address,
    pub total_deposited: i128,
    pub total_released: i128,
    pub milestones_completed: u32,
    pub cancelled: bool,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Vault(u64),
    Donation(u64, Address),
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    VaultNotFound = 3,
}

/// Approximate ledgers per day at a 5-second close time. Used to express
/// storage TTLs (which the network counts in ledgers, not wall time) in
/// human terms.
const DAY_IN_LEDGERS: u32 = 17_280;

const INSTANCE_BUMP_AMOUNT: u32 = 30 * DAY_IN_LEDGERS;
const INSTANCE_LIFETIME_THRESHOLD: u32 = INSTANCE_BUMP_AMOUNT - DAY_IN_LEDGERS;

fn extend_instance_ttl(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
}

#[contract]
pub struct MilestoneVault;

#[contractimpl]
impl MilestoneVault {
    /// Sets the vault admin. Can only be called once.
    pub fn init(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        extend_instance_ttl(&env);
        Ok(())
    }

    /// Reads back the vault admin set by `init`.
    pub fn admin(env: Env) -> Result<Address, Error> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)
    }

    /// Reads back a project's pooled-donation vault by id.
    pub fn get_vault(env: Env, project_id: u64) -> Result<ProjectVault, Error> {
        env.storage()
            .persistent()
            .get(&DataKey::Vault(project_id))
            .ok_or(Error::VaultNotFound)
    }
}

mod test;
