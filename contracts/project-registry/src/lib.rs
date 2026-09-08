#![no_std]

use soroban_sdk::{
    contract, contracterror, contracttype, contractimpl, symbol_short, Address, BytesN, Env,
    String,
};

/// A reforestation project registered with the platform.
///
/// `polygon_hash` commits on-chain to the GPS-bounded plot boundary (the
/// actual GeoJSON polygon lives off-chain in the backend, keyed by this
/// hash) so the plot a project is scored against can't be silently swapped
/// after donors have funded it. `attestor` is the address authorized to
/// submit milestone confirmations for this project on the milestone-vault
/// contract.
#[contracttype]
#[derive(Clone)]
pub struct Project {
    pub operator: Address,
    pub recipient: Address,
    pub attestor: Address,
    pub polygon_hash: BytesN<32>,
    pub name: String,
    pub approved: bool,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    NextProjectId,
    Project(u64),
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    ProjectNotFound = 3,
}

/// Approximate ledgers per day at a 5-second close time. Used to express
/// storage TTLs (which the network counts in ledgers, not wall time) in
/// human terms.
const DAY_IN_LEDGERS: u32 = 17_280;

const INSTANCE_BUMP_AMOUNT: u32 = 30 * DAY_IN_LEDGERS;
const INSTANCE_LIFETIME_THRESHOLD: u32 = INSTANCE_BUMP_AMOUNT - DAY_IN_LEDGERS;

const PROJECT_BUMP_AMOUNT: u32 = 90 * DAY_IN_LEDGERS;
const PROJECT_LIFETIME_THRESHOLD: u32 = PROJECT_BUMP_AMOUNT - DAY_IN_LEDGERS;

fn extend_instance_ttl(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
}

fn extend_project_ttl(env: &Env, project_id: u64) {
    env.storage().persistent().extend_ttl(
        &DataKey::Project(project_id),
        PROJECT_LIFETIME_THRESHOLD,
        PROJECT_BUMP_AMOUNT,
    );
}

#[contract]
pub struct ProjectRegistry;

#[contractimpl]
impl ProjectRegistry {
    /// Sets the registry admin and seeds the project-id counter. Can only
    /// be called once.
    pub fn init(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::NextProjectId, &0u64);
        extend_instance_ttl(&env);
        Ok(())
    }

    /// Reads back the registry admin set by `init`.
    pub fn admin(env: Env) -> Result<Address, Error> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)
    }

    /// Reads back a project by id, registered or not yet approved.
    pub fn get_project(env: Env, project_id: u64) -> Result<Project, Error> {
        env.storage()
            .persistent()
            .get(&DataKey::Project(project_id))
            .ok_or(Error::ProjectNotFound)
    }

    /// Registers a new reforestation project. Callable by the operator
    /// that will manage it. The project starts unapproved — donors can't
    /// fund it on the milestone-vault until an admin approves it here.
    pub fn register(
        env: Env,
        operator: Address,
        recipient: Address,
        attestor: Address,
        polygon_hash: BytesN<32>,
        name: String,
    ) -> Result<u64, Error> {
        operator.require_auth();

        let project_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::NextProjectId)
            .unwrap_or(0);

        let project = Project {
            operator: operator.clone(),
            recipient,
            attestor,
            polygon_hash,
            name: name.clone(),
            approved: false,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Project(project_id), &project);
        env.storage()
            .instance()
            .set(&DataKey::NextProjectId, &(project_id + 1));

        extend_instance_ttl(&env);
        extend_project_ttl(&env, project_id);

        env.events()
            .publish((symbol_short!("register"), project_id), (operator, name));

        Ok(project_id)
    }

    /// Marks a registered project as approved, allowing donors to fund it
    /// on the milestone-vault contract. Admin-only.
    pub fn approve_project(env: Env, project_id: u64) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        let key = DataKey::Project(project_id);
        let mut project: Project = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(Error::ProjectNotFound)?;
        project.approved = true;
        env.storage().persistent().set(&key, &project);
        extend_instance_ttl(&env);
        extend_project_ttl(&env, project_id);

        env.events()
            .publish((symbol_short!("approved"), project_id), ());

        Ok(())
    }
}

mod test;
