#![cfg(test)]

use super::*;
use soroban_sdk::testutils::Address as _;

fn setup() -> (Env, ProjectRegistryClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(ProjectRegistry, ());
    let client = ProjectRegistryClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.init(&admin);

    (env, client, admin)
}

#[test]
fn init_sets_admin() {
    let (_env, client, admin) = setup();
    assert_eq!(client.admin(), admin);
}

#[test]
fn double_init_fails() {
    let (_env, client, admin) = setup();
    let result = client.try_init(&admin);
    assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
}

#[test]
fn get_unregistered_project_fails() {
    let (_env, client, _admin) = setup();
    let result = client.try_get_project(&0u64);
    assert_eq!(result, Err(Ok(Error::ProjectNotFound)));
}

fn dummy_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[7u8; 32])
}

#[test]
fn register_project_stores_unapproved_entry() {
    let (env, client, _admin) = setup();
    let operator = Address::generate(&env);
    let recipient = Address::generate(&env);
    let attestor = Address::generate(&env);
    let name = String::from_str(&env, "Kakamega Forest Restoration");
    let polygon_hash = dummy_hash(&env);

    let project_id = client.register(&operator, &recipient, &attestor, &polygon_hash, &name);
    assert_eq!(project_id, 0);

    let project = client.get_project(&project_id);
    assert_eq!(project.operator, operator);
    assert_eq!(project.recipient, recipient);
    assert_eq!(project.attestor, attestor);
    assert_eq!(project.name, name);
    assert_eq!(project.approved, false);
}

#[test]
fn register_assigns_sequential_ids() {
    let (env, client, _admin) = setup();
    let operator = Address::generate(&env);
    let recipient = Address::generate(&env);
    let attestor = Address::generate(&env);
    let name = String::from_str(&env, "Plot");
    let polygon_hash = dummy_hash(&env);

    let first = client.register(&operator, &recipient, &attestor, &polygon_hash, &name);
    let second = client.register(&operator, &recipient, &attestor, &polygon_hash, &name);
    assert_eq!(first, 0);
    assert_eq!(second, 1);
}

#[test]
fn approve_project_marks_approved() {
    let (env, client, _admin) = setup();
    let operator = Address::generate(&env);
    let recipient = Address::generate(&env);
    let attestor = Address::generate(&env);
    let name = String::from_str(&env, "Plot");
    let polygon_hash = dummy_hash(&env);
    let project_id = client.register(&operator, &recipient, &attestor, &polygon_hash, &name);

    client.approve_project(&project_id);

    let project = client.get_project(&project_id);
    assert!(project.approved);
}

#[test]
fn approve_unregistered_project_fails() {
    let (_env, client, _admin) = setup();
    let result = client.try_approve_project(&99u64);
    assert_eq!(result, Err(Ok(Error::ProjectNotFound)));
}
