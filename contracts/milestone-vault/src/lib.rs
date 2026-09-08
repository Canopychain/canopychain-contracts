#![no_std]

use soroban_sdk::{
    contract, contracterror, contracttype, contractimpl, symbol_short, token, Address, Env, Vec,
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

/// One tranche of a project's release schedule.
///
/// `threshold_bps` is the cumulative forest-cover-change (in basis points
/// of plot area) the backend's satellite check must confirm before the
/// attestor will attest this milestone; it's recorded on-chain purely for
/// donor-facing transparency and isn't checked by the contract itself,
/// which has no way to verify satellite data. `payout_bps` is the share of
/// `total_deposited` released to the recipient when this milestone is
/// attested.
#[contracttype]
#[derive(Clone)]
pub struct Milestone {
    pub threshold_bps: u32,
    pub payout_bps: u32,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Vault(u64),
    Donation(u64, Address),
    Schedule(u64),
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
    ScheduleAlreadySet = 7,
    InvalidSchedule = 8,
    ScheduleNotFound = 9,
    AllMilestonesComplete = 10,
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

fn extend_schedule_ttl(env: &Env, project_id: u64) {
    env.storage().persistent().extend_ttl(
        &DataKey::Schedule(project_id),
        VAULT_LIFETIME_THRESHOLD,
        VAULT_BUMP_AMOUNT,
    );
}

/// Basis-points denominator (10_000 bps = 100%).
const BPS_DENOMINATOR: u32 = 10_000;

/// Rejects an empty schedule, a schedule whose payouts sum to more than
/// 100%, or one whose thresholds don't strictly increase (a project should
/// need more forest-cover change to unlock each later tranche, never less
/// or the same).
fn validate_schedule(milestones: &Vec<Milestone>) -> Result<(), Error> {
    if milestones.is_empty() {
        return Err(Error::InvalidSchedule);
    }

    let mut payout_total: u32 = 0;
    let mut prev_threshold: Option<u32> = None;
    for milestone in milestones.iter() {
        if let Some(prev) = prev_threshold {
            if milestone.threshold_bps <= prev {
                return Err(Error::InvalidSchedule);
            }
        }
        prev_threshold = Some(milestone.threshold_bps);

        payout_total = match payout_total.checked_add(milestone.payout_bps) {
            Some(total) => total,
            None => return Err(Error::InvalidSchedule),
        };
    }

    if payout_total > BPS_DENOMINATOR {
        return Err(Error::InvalidSchedule);
    }

    Ok(())
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

    /// Sets a project's tranche-release schedule. Admin-only, and callable
    /// only once per project — the schedule donors funded against can't be
    /// quietly changed underneath them after the fact.
    pub fn configure_milestones(
        env: Env,
        project_id: u64,
        milestones: Vec<Milestone>,
    ) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        let key = DataKey::Schedule(project_id);
        if env.storage().persistent().has(&key) {
            return Err(Error::ScheduleAlreadySet);
        }

        validate_schedule(&milestones)?;

        env.storage().persistent().set(&key, &milestones);
        extend_instance_ttl(&env);
        extend_schedule_ttl(&env, project_id);

        env.events()
            .publish((symbol_short!("schedule"), project_id), milestones.len());

        Ok(())
    }

    /// Reads back a project's tranche-release schedule.
    pub fn get_schedule(env: Env, project_id: u64) -> Result<Vec<Milestone>, Error> {
        env.storage()
            .persistent()
            .get(&DataKey::Schedule(project_id))
            .ok_or(Error::ScheduleNotFound)
    }

    /// Confirms that the next milestone in a project's schedule has been
    /// reached. Attestor-gated — the attestor recorded against the vault
    /// (set on the project's first deposit) is the only address that can
    /// call this. Releases that tranche's share of `total_deposited` to
    /// the recipient and advances `milestones_completed` by one.
    pub fn attest_milestone(env: Env, project_id: u64) -> Result<i128, Error> {
        let vault_key = DataKey::Vault(project_id);
        let mut vault: ProjectVault = env
            .storage()
            .persistent()
            .get(&vault_key)
            .ok_or(Error::VaultNotFound)?;

        vault.attestor.require_auth();

        if vault.cancelled {
            return Err(Error::VaultCancelled);
        }

        let schedule: Vec<Milestone> = env
            .storage()
            .persistent()
            .get(&DataKey::Schedule(project_id))
            .ok_or(Error::ScheduleNotFound)?;

        if vault.milestones_completed >= schedule.len() {
            return Err(Error::AllMilestonesComplete);
        }

        let milestone = schedule
            .get(vault.milestones_completed)
            .ok_or(Error::AllMilestonesComplete)?;

        let payout =
            (vault.total_deposited * milestone.payout_bps as i128) / BPS_DENOMINATOR as i128;

        vault.milestones_completed += 1;
        vault.total_released += payout;
        env.storage().persistent().set(&vault_key, &vault);

        extend_instance_ttl(&env);
        extend_vault_ttl(&env, project_id);

        if payout > 0 {
            let token_client = token::Client::new(&env, &vault.token);
            token_client.transfer(&env.current_contract_address(), &vault.recipient, &payout);
        }

        env.events().publish(
            (
                symbol_short!("attested"),
                project_id,
                vault.milestones_completed,
            ),
            payout,
        );

        Ok(payout)
    }
}

mod test;
