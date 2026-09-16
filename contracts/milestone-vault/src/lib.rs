#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, Env, Vec,
};

mod math;

/// The pooled donor funds for a single reforestation project.
///
/// Donors deposit into the pool named by `project_id`; there is no
/// per-donor accrual rate to track like a payment stream — instead, a
/// fraction of `total_deposited` releases to `recipient` each time the
/// `attestor` confirms a milestone, per the project's milestone schedule.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
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
#[derive(Clone, Debug, PartialEq)]
pub struct Milestone {
    pub threshold_bps: u32,
    pub payout_bps: u32,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// The admin address. Instance storage.
    Admin,
    /// A project's pooled donor funds, keyed by project id. Persistent storage.
    Vault(u64),
    /// A single donor's contribution to a project, keyed by
    /// `(project_id, donor)`. Persistent storage.
    Donation(u64, Address),
    /// A project's milestone release schedule, keyed by project id.
    /// Persistent storage.
    Schedule(u64),
    /// Whether the contract is paused. Instance storage.
    Paused,
    /// The minimum accepted deposit amount, in the deposited token's
    /// smallest unit. Instance storage.
    MinDeposit,
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
    ContractPaused = 11,
    NotCancelled = 12,
    NothingToRefund = 13,
    DepositBelowMinimum = 14,
}

/// Approximate ledgers per day at a 5-second close time. Used to express
/// storage TTLs (which the network counts in ledgers, not wall time) in
/// human terms.
const DAY_IN_LEDGERS: u32 = 17_280;

const INSTANCE_BUMP_AMOUNT: u32 = 30 * DAY_IN_LEDGERS;
const INSTANCE_LIFETIME_THRESHOLD: u32 = INSTANCE_BUMP_AMOUNT - DAY_IN_LEDGERS;

const VAULT_BUMP_AMOUNT: u32 = 90 * DAY_IN_LEDGERS;
const VAULT_LIFETIME_THRESHOLD: u32 = VAULT_BUMP_AMOUNT - DAY_IN_LEDGERS;

/// Keeps the contract instance (admin, pause flag) from being archived.
/// Called on every state-changing entry point.
fn extend_instance_ttl(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
}

/// Keeps a project's vault entry alive past its last touch, so a
/// slow-progressing project doesn't get archived out from under its
/// donors and recipient between activity.
fn extend_vault_ttl(env: &Env, project_id: u64) {
    env.storage().persistent().extend_ttl(
        &DataKey::Vault(project_id),
        VAULT_LIFETIME_THRESHOLD,
        VAULT_BUMP_AMOUNT,
    );
}

/// Keeps a donor's per-project contribution record alive past its last
/// touch, so it's still there to compute a refund share against if the
/// project is later cancelled.
fn extend_donation_ttl(env: &Env, project_id: u64, donor: &Address) {
    env.storage().persistent().extend_ttl(
        &DataKey::Donation(project_id, donor.clone()),
        VAULT_LIFETIME_THRESHOLD,
        VAULT_BUMP_AMOUNT,
    );
}

/// Keeps a project's milestone schedule alive past its last touch.
fn extend_schedule_ttl(env: &Env, project_id: u64) {
    env.storage().persistent().extend_ttl(
        &DataKey::Schedule(project_id),
        VAULT_LIFETIME_THRESHOLD,
        VAULT_BUMP_AMOUNT,
    );
}

/// Returns `Err(Error::ContractPaused)` if an admin has paused the vault.
/// Checked at the top of every entry point that moves funds.
fn require_not_paused(env: &Env) -> Result<(), Error> {
    let paused: bool = env
        .storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false);
    if paused {
        return Err(Error::ContractPaused);
    }
    Ok(())
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

// Events still go out through env.events().publish rather than the
// #[contractevent] macro the SDK now prefers: the tuple-topic layout is a
// published interface (EVENTS.md) that the backend indexer decodes
// positionally, so switching encodings is a coordinated change across both
// repos, not a local one.
#[allow(deprecated)]
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

    /// Rotates the admin address. Gated by the current admin, since the
    /// admin key is the system's only circuit breaker — it's what gates
    /// `pause`, `configure_milestones`, `set_attestor`, and
    /// `cancel_project` — losing it without a way to rotate it would mean
    /// losing the ability to halt a stalled project or replace a
    /// compromised attestor.
    pub fn transfer_admin(env: Env, new_admin: Address) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        env.storage().instance().set(&DataKey::Admin, &new_admin);
        extend_instance_ttl(&env);

        env.events().publish((symbol_short!("admin"),), new_admin);

        Ok(())
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
        require_not_paused(&env)?;
        donor.require_auth();

        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let min_deposit: i128 = env
            .storage()
            .instance()
            .get(&DataKey::MinDeposit)
            .unwrap_or(0);
        if amount < min_deposit {
            return Err(Error::DepositBelowMinimum);
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
        token_client.transfer(&donor, env.current_contract_address(), &amount);

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

    /// Sets a project's tranche-release schedule. Admin-only. Freely
    /// reconfigurable — including overwriting a prior schedule outright —
    /// up until the project's first deposit lands; there's no donor to
    /// protect from a changing schedule before then, so a typo'd schedule
    /// isn't permanent. Once a donor has funded the project, the schedule
    /// locks and further calls fail.
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
        let has_deposits = env.storage().persistent().has(&DataKey::Vault(project_id));
        if env.storage().persistent().has(&key) && has_deposits {
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

    /// Reads back the next milestone a project is waiting on — the entry
    /// in its schedule at index `milestones_completed` — or
    /// `Error::AllMilestonesComplete` once every tranche has been attested.
    /// Collapses the get_vault + get_schedule + index pattern a caller
    /// otherwise has to repeat on every poll into a single read.
    pub fn next_milestone(env: Env, project_id: u64) -> Result<Milestone, Error> {
        let vault: ProjectVault = env
            .storage()
            .persistent()
            .get(&DataKey::Vault(project_id))
            .ok_or(Error::VaultNotFound)?;

        let schedule: Vec<Milestone> = env
            .storage()
            .persistent()
            .get(&DataKey::Schedule(project_id))
            .ok_or(Error::ScheduleNotFound)?;

        schedule
            .get(vault.milestones_completed)
            .ok_or(Error::AllMilestonesComplete)
    }

    /// Confirms that the next milestone in a project's schedule has been
    /// reached. Attestor-gated — the attestor recorded against the vault
    /// (set on the project's first deposit) is the only address that can
    /// call this. Releases that tranche's share of `total_deposited` to
    /// the recipient and advances `milestones_completed` by one.
    pub fn attest_milestone(env: Env, project_id: u64) -> Result<i128, Error> {
        require_not_paused(&env)?;

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

        let payout = math::tranche_payout(vault.total_deposited, milestone.payout_bps);

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

    /// Reads back the minimum accepted deposit amount. Defaults to 0 (no
    /// floor) until an admin sets one with `set_min_deposit`.
    pub fn min_deposit(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::MinDeposit)
            .unwrap_or(0)
    }

    /// Sets the minimum amount `deposit` will accept. Admin-gated, so
    /// whoever runs a given instance can pick a floor that fits the token
    /// and project sizes it actually handles, rather than the contract
    /// hard-coding one that fits none of them. A dust-sized contribution
    /// below the floor would round to nothing in every tranche calculation
    /// it takes part in while still paying storage rent on its donation
    /// record, which this exists to avoid.
    pub fn set_min_deposit(env: Env, min_deposit: i128) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        if min_deposit < 0 {
            return Err(Error::InvalidAmount);
        }

        env.storage()
            .instance()
            .set(&DataKey::MinDeposit, &min_deposit);
        extend_instance_ttl(&env);

        env.events()
            .publish((symbol_short!("mindep"),), min_deposit);

        Ok(())
    }

    /// Halts deposits and milestone attestations. Admin-gated emergency
    /// brake. Existing vault balances and schedules are untouched, and
    /// reads keep working — this only blocks fund movement.
    pub fn pause(env: Env) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        env.storage().instance().set(&DataKey::Paused, &true);
        extend_instance_ttl(&env);
        env.events().publish((symbol_short!("pause"),), ());
        Ok(())
    }

    /// Lifts a pause, restoring normal operation. Admin-gated.
    pub fn unpause(env: Env) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        env.storage().instance().set(&DataKey::Paused, &false);
        extend_instance_ttl(&env);
        env.events().publish((symbol_short!("unpause"),), ());
        Ok(())
    }

    /// Whether the vault is currently paused.
    pub fn paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    /// Rotates the attestor authorized to attest milestones for a project.
    /// Admin-gated, so a compromised or retiring attestor key can be
    /// replaced without needing anything from donors or the recipient.
    pub fn set_attestor(env: Env, project_id: u64, new_attestor: Address) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        let key = DataKey::Vault(project_id);
        let mut vault: ProjectVault = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(Error::VaultNotFound)?;

        vault.attestor = new_attestor;
        env.storage().persistent().set(&key, &vault);
        extend_instance_ttl(&env);
        extend_vault_ttl(&env, project_id);

        env.events()
            .publish((symbol_short!("attestor"), project_id), ());

        Ok(())
    }

    /// Cancels a project's vault. Admin-gated — this is for projects that
    /// stall or fail to progress (satellite data never confirms growth, the
    /// operator abandons the plot), not something a single donor can
    /// trigger unilaterally against a pool other donors also contributed
    /// to. Blocks further deposits and attestations; whatever hasn't been
    /// released yet becomes claimable by donors via `refund`.
    pub fn cancel_project(env: Env, project_id: u64) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        let key = DataKey::Vault(project_id);
        let mut vault: ProjectVault = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(Error::VaultNotFound)?;

        vault.cancelled = true;
        env.storage().persistent().set(&key, &vault);
        extend_instance_ttl(&env);
        extend_vault_ttl(&env, project_id);

        env.events()
            .publish((symbol_short!("cancelled"), project_id), ());

        Ok(())
    }

    /// Claims a donor's share of what's left in a cancelled project's pool.
    /// Donor-auth-gated, and pull-based (each donor claims their own share
    /// rather than the contract pushing to everyone at once) since there's
    /// no way to enumerate every donor to a project in one call. The share
    /// is `donation / total_deposited` of whatever wasn't released before
    /// cancellation, so it doesn't matter what order donors claim in — the
    /// shares always add up to what's actually left. Zeroes the donor's
    /// recorded donation on success so a second call has nothing to pay.
    pub fn refund(env: Env, project_id: u64, donor: Address) -> Result<i128, Error> {
        donor.require_auth();

        let vault: ProjectVault = env
            .storage()
            .persistent()
            .get(&DataKey::Vault(project_id))
            .ok_or(Error::VaultNotFound)?;

        if !vault.cancelled {
            return Err(Error::NotCancelled);
        }

        let donation_key = DataKey::Donation(project_id, donor.clone());
        let donation: i128 = env.storage().persistent().get(&donation_key).unwrap_or(0);
        if donation <= 0 {
            return Err(Error::NothingToRefund);
        }

        let remaining = vault.total_deposited - vault.total_released;
        let refund_amount = math::proportional_share(donation, remaining, vault.total_deposited);

        env.storage().persistent().set(&donation_key, &0i128);
        extend_instance_ttl(&env);
        extend_donation_ttl(&env, project_id, &donor);

        if refund_amount > 0 {
            let token_client = token::Client::new(&env, &vault.token);
            token_client.transfer(&env.current_contract_address(), &donor, &refund_amount);
        }

        env.events()
            .publish((symbol_short!("refund"), project_id, donor), refund_amount);

        Ok(refund_amount)
    }
}

mod test;
