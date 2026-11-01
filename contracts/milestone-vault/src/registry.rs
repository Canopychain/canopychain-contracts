//! The slice of project-registry this vault reads when a project's vault
//! opens. Declared here rather than imported from the registry crate, which
//! would link the registry's own entry points into this contract's wasm.

use soroban_sdk::{contractclient, contracttype, Address, BytesN, Env, String};

/// Mirrors project-registry's `Project` field for field, so its stored value
/// decodes into this type. Not exported into this contract's interface,
/// since no vault entry point takes or returns it.
#[contracttype(export = false)]
#[derive(Clone, Debug, PartialEq)]
pub struct Project {
    pub operator: Address,
    pub recipient: Address,
    pub attestor: Address,
    pub polygon_hash: BytesN<32>,
    pub name: String,
    pub approved: bool,
}

#[allow(dead_code)]
#[contractclient(name = "RegistryClient")]
pub trait Registry {
    fn get_project(env: Env, project_id: u64) -> Project;
}
