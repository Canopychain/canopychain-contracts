#![cfg(test)]

use super::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient};

fn create_token<'a>(env: &Env, admin: &Address) -> (TokenClient<'a>, StellarAssetClient<'a>) {
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    (
        TokenClient::new(env, &sac.address()),
        StellarAssetClient::new(env, &sac.address()),
    )
}

struct Setup<'a> {
    env: Env,
    client: MilestoneVaultClient<'a>,
    admin: Address,
    token: TokenClient<'a>,
    token_admin: StellarAssetClient<'a>,
    donor: Address,
    recipient: Address,
    attestor: Address,
}

fn setup() -> Setup<'static> {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(MilestoneVault, ());
    let client = MilestoneVaultClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.init(&admin);

    let token_issuer = Address::generate(&env);
    let (token, token_admin) = create_token(&env, &token_issuer);

    let donor = Address::generate(&env);
    let recipient = Address::generate(&env);
    let attestor = Address::generate(&env);

    Setup {
        env,
        client,
        admin,
        token,
        token_admin,
        donor,
        recipient,
        attestor,
    }
}

#[test]
fn init_sets_admin() {
    let s = setup();
    assert_eq!(s.client.admin(), s.admin);
}

#[test]
fn double_init_fails() {
    let s = setup();
    let result = s.client.try_init(&s.admin);
    assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
}

#[test]
fn get_unknown_vault_fails() {
    let s = setup();
    let result = s.client.try_get_vault(&0u64);
    assert_eq!(result, Err(Ok(Error::VaultNotFound)));
}

#[test]
fn first_deposit_opens_vault() {
    let s = setup();
    s.token_admin.mint(&s.donor, &1_000);

    let total = s.client.deposit(
        &s.donor,
        &0u64,
        &s.recipient,
        &s.attestor,
        &s.token.address,
        &600,
    );
    assert_eq!(total, 600);

    let vault = s.client.get_vault(&0u64);
    assert_eq!(vault.recipient, s.recipient);
    assert_eq!(vault.attestor, s.attestor);
    assert_eq!(vault.total_deposited, 600);
    assert_eq!(vault.total_released, 0);
    assert_eq!(vault.milestones_completed, 0);
    assert_eq!(vault.cancelled, false);

    assert_eq!(s.token.balance(&s.donor), 400);
    assert_eq!(s.token.balance(&s.client.address), 600);
    assert_eq!(s.client.get_donation(&0u64, &s.donor), 600);
}

#[test]
fn second_deposit_tops_up_and_ignores_new_recipient_attestor() {
    let s = setup();
    s.token_admin.mint(&s.donor, &1_000);

    s.client.deposit(
        &s.donor,
        &0u64,
        &s.recipient,
        &s.attestor,
        &s.token.address,
        &400,
    );

    let other_recipient = Address::generate(&s.env);
    let other_attestor = Address::generate(&s.env);
    s.client.deposit(
        &s.donor,
        &0u64,
        &other_recipient,
        &other_attestor,
        &s.token.address,
        &200,
    );

    let vault = s.client.get_vault(&0u64);
    assert_eq!(vault.total_deposited, 600);
    assert_eq!(vault.recipient, s.recipient);
    assert_eq!(vault.attestor, s.attestor);
    assert_eq!(s.client.get_donation(&0u64, &s.donor), 600);
}

#[test]
fn deposit_rejects_non_positive_amount() {
    let s = setup();
    s.token_admin.mint(&s.donor, &1_000);

    let result = s.client.try_deposit(
        &s.donor,
        &0u64,
        &s.recipient,
        &s.attestor,
        &s.token.address,
        &0,
    );
    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn deposit_rejects_token_mismatch_on_top_up() {
    let s = setup();
    s.token_admin.mint(&s.donor, &1_000);

    s.client.deposit(
        &s.donor,
        &0u64,
        &s.recipient,
        &s.attestor,
        &s.token.address,
        &100,
    );

    let other_issuer = Address::generate(&s.env);
    let (other_token, other_token_admin) = create_token(&s.env, &other_issuer);
    other_token_admin.mint(&s.donor, &1_000);

    let result = s.client.try_deposit(
        &s.donor,
        &0u64,
        &s.recipient,
        &s.attestor,
        &other_token.address,
        &100,
    );
    assert_eq!(result, Err(Ok(Error::TokenMismatch)));
}

#[test]
fn deposits_from_different_donors_pool_together() {
    let s = setup();
    let donor_two = Address::generate(&s.env);
    s.token_admin.mint(&s.donor, &1_000);
    s.token_admin.mint(&donor_two, &1_000);

    s.client.deposit(
        &s.donor,
        &0u64,
        &s.recipient,
        &s.attestor,
        &s.token.address,
        &300,
    );
    s.client.deposit(
        &donor_two,
        &0u64,
        &s.recipient,
        &s.attestor,
        &s.token.address,
        &500,
    );

    let vault = s.client.get_vault(&0u64);
    assert_eq!(vault.total_deposited, 800);
    assert_eq!(s.client.get_donation(&0u64, &s.donor), 300);
    assert_eq!(s.client.get_donation(&0u64, &donor_two), 500);
}

fn three_tranche_schedule(env: &Env) -> Vec<Milestone> {
    Vec::from_array(
        env,
        [
            Milestone {
                threshold_bps: 500,
                payout_bps: 3_000,
            },
            Milestone {
                threshold_bps: 1_000,
                payout_bps: 3_000,
            },
            Milestone {
                threshold_bps: 2_000,
                payout_bps: 4_000,
            },
        ],
    )
}

#[test]
fn configure_milestones_stores_schedule() {
    let s = setup();
    let schedule = three_tranche_schedule(&s.env);

    s.client.configure_milestones(&0u64, &schedule);

    let stored = s.client.get_schedule(&0u64);
    assert_eq!(stored.len(), 3);
    assert_eq!(stored.get(0).unwrap().payout_bps, 3_000);
    assert_eq!(stored.get(2).unwrap().threshold_bps, 2_000);
}

#[test]
fn configure_milestones_twice_fails() {
    let s = setup();
    let schedule = three_tranche_schedule(&s.env);

    s.client.configure_milestones(&0u64, &schedule);
    let result = s.client.try_configure_milestones(&0u64, &schedule);
    assert_eq!(result, Err(Ok(Error::ScheduleAlreadySet)));
}

#[test]
fn get_schedule_before_configure_fails() {
    let s = setup();
    let result = s.client.try_get_schedule(&0u64);
    assert_eq!(result, Err(Ok(Error::ScheduleNotFound)));
}

#[test]
fn configure_milestones_rejects_empty_schedule() {
    let s = setup();
    let empty: Vec<Milestone> = Vec::new(&s.env);
    let result = s.client.try_configure_milestones(&0u64, &empty);
    assert_eq!(result, Err(Ok(Error::InvalidSchedule)));
}

#[test]
fn configure_milestones_rejects_payouts_over_100_percent() {
    let s = setup();
    let schedule = Vec::from_array(
        &s.env,
        [
            Milestone {
                threshold_bps: 500,
                payout_bps: 6_000,
            },
            Milestone {
                threshold_bps: 1_000,
                payout_bps: 5_000,
            },
        ],
    );
    let result = s.client.try_configure_milestones(&0u64, &schedule);
    assert_eq!(result, Err(Ok(Error::InvalidSchedule)));
}

#[test]
fn configure_milestones_rejects_non_increasing_thresholds() {
    let s = setup();
    let schedule = Vec::from_array(
        &s.env,
        [
            Milestone {
                threshold_bps: 1_000,
                payout_bps: 3_000,
            },
            Milestone {
                threshold_bps: 1_000,
                payout_bps: 3_000,
            },
        ],
    );
    let result = s.client.try_configure_milestones(&0u64, &schedule);
    assert_eq!(result, Err(Ok(Error::InvalidSchedule)));
}

#[test]
fn attest_milestone_advances_count() {
    let s = setup();
    s.token_admin.mint(&s.donor, &1_000);
    s.client.deposit(
        &s.donor,
        &0u64,
        &s.recipient,
        &s.attestor,
        &s.token.address,
        &1_000,
    );
    s.client
        .configure_milestones(&0u64, &three_tranche_schedule(&s.env));

    let completed = s.client.attest_milestone(&0u64);
    assert_eq!(completed, 1);

    let vault = s.client.get_vault(&0u64);
    assert_eq!(vault.milestones_completed, 1);

    let completed = s.client.attest_milestone(&0u64);
    assert_eq!(completed, 2);
}

#[test]
fn attest_milestone_without_vault_fails() {
    let s = setup();
    let result = s.client.try_attest_milestone(&0u64);
    assert_eq!(result, Err(Ok(Error::VaultNotFound)));
}

#[test]
fn attest_milestone_without_schedule_fails() {
    let s = setup();
    s.token_admin.mint(&s.donor, &1_000);
    s.client.deposit(
        &s.donor,
        &0u64,
        &s.recipient,
        &s.attestor,
        &s.token.address,
        &1_000,
    );

    let result = s.client.try_attest_milestone(&0u64);
    assert_eq!(result, Err(Ok(Error::ScheduleNotFound)));
}

#[test]
fn attest_milestone_past_schedule_end_fails() {
    let s = setup();
    s.token_admin.mint(&s.donor, &1_000);
    s.client.deposit(
        &s.donor,
        &0u64,
        &s.recipient,
        &s.attestor,
        &s.token.address,
        &1_000,
    );
    s.client
        .configure_milestones(&0u64, &three_tranche_schedule(&s.env));

    s.client.attest_milestone(&0u64);
    s.client.attest_milestone(&0u64);
    s.client.attest_milestone(&0u64);

    let result = s.client.try_attest_milestone(&0u64);
    assert_eq!(result, Err(Ok(Error::AllMilestonesComplete)));
}
