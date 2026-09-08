#![no_std]

use soroban_sdk::{
    contract, contracterror, contracttype, contractimpl, symbol_short, token, Address, Env,
};

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
    InvalidAmount = 4,
    TokenMismatch = 5,
    VaultCancelled = 6,
}

/// Approximate ledgers per day at a 5-second close time. Used to express
/// storage TTLs (which the network counts in ledgers, not wall time) in
/// human terms.
const DAY_IN_LEDGERS: u32 = 17_280;

const INSTANCE_BUMP_AMOUNT: u32 = 30 * DAY_IN_LEDGERS;
const INSTANCE_LIFETIME_THRESHOLD: u32 = INSTANCE_BUMP_AMOUNT - DAY_IN_LEDGERS;

const VAULT_BUMP_AMOUNT: u32 = 90 * DAY_IN_LEDGERS;
const VAULT_LIFETIME_THRESHOLD: u32 = VAULT_BUMP_AMOUNT - DAY_IN_LEDGERS;

fn extend_instance_ttl(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
}

fn extend_vault_ttl(env: &Env, project_id: u64) {
    env.storage().persistent().extend_ttl(
        &DataKey::Vault(project_id),
        VAULT_LIFETIME_THRESHOLD,
        VAULT_BUMP_AMOUNT,
    );
}

fn extend_donation_ttl(env: &Env, project_id: u64, donor: &Address) {
    env.storage().persistent().extend_ttl(
        &DataKey::Donation(project_id, donor.clone()),
        VAULT_LIFETIME_THRESHOLD,
        VAULT_BUMP_AMOUNT,
    );
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

    /// Reads back how much a given donor has contributed to a project.
    pub fn get_donation(env: Env, project_id: u64, donor: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Donation(project_id, donor))
            .unwrap_or(0)
    }

    /// Deposits `amount` of `token` from `donor` into a project's pool.
    /// The first deposit for a `project_id` opens its vault, recording
    /// `recipient` and `attestor` for that project; later deposits into the
    /// same project ignore those two arguments and top up the existing
    /// vault instead, so they can't be used to redirect an already-funded
    /// project's payout or attestation rights.
    pub fn deposit(
        env: Env,
        donor: Address,
        project_id: u64,
        recipient: Address,
        attestor: Address,
        token: Address,
        amount: i128,
    ) -> Result<i128, Error> {
        donor.require_auth();

        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let key = DataKey::Vault(project_id);
        let mut vault: ProjectVault = match env.storage().persistent().get(&key) {
            Some(vault) => vault,
            None => ProjectVault {
                recipient,
                attestor,
                token: token.clone(),
                total_deposited: 0,
                total_released: 0,
                milestones_completed: 0,
                cancelled: false,
            },
        };

        if vault.cancelled {
            return Err(Error::VaultCancelled);
        }
        if vault.token != token {
            return Err(Error::TokenMismatch);
        }

        let token_client = token::Client::new(&env, &token);
        token_client.transfer(&donor, &env.current_contract_address(), &amount);

        vault.total_deposited += amount;
        env.storage().persistent().set(&key, &vault);

        let donation_key = DataKey::Donation(project_id, donor.clone());
        let prior: i128 = env.storage().persistent().get(&donation_key).unwrap_or(0);
        env.storage()
            .persistent()
            .set(&donation_key, &(prior + amount));

        extend_instance_ttl(&env);
        extend_vault_ttl(&env, project_id);
        extend_donation_ttl(&env, project_id, &donor);

        env.events().publish(
            (symbol_short!("deposit"), project_id, donor),
            (amount, vault.total_deposited),
        );

        Ok(vault.total_deposited)
    }
}

mod test;
