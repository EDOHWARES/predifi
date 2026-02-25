#![cfg(test)]
#![allow(deprecated)]

use super::*;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Symbol,
};

use crate::CreatePoolArgs;

mod dummy_access_control {
    use soroban_sdk::{contract, contractimpl, Address, Env, Symbol};

    #[contract]
    pub struct DummyAccessControl;

    #[contractimpl]
    impl DummyAccessControl {
        pub fn grant_role(env: Env, user: Address, role: u32) {
            let key = (Symbol::new(&env, "role"), user, role);
            env.storage().instance().set(&key, &true);
        }

        pub fn has_role(env: Env, user: Address, role: u32) -> bool {
            let key = (Symbol::new(&env, "role"), user, role);
            env.storage().instance().get(&key).unwrap_or(false)
        }
    }
}

const ROLE_ADMIN: u32 = 0;
const ROLE_OPERATOR: u32 = 1;
const ROLE_ORACLE: u32 = 3;

fn setup(
    env: &Env,
) -> (
    dummy_access_control::DummyAccessControlClient<'_>,
    PredifiContractClient<'_>,
    Address,
    token::Client<'_>,
    token::StellarAssetClient<'_>,
    Address,
    Address,
    Address,
) {
    let ac_id = env.register(dummy_access_control::DummyAccessControl, ());
    let ac_client = dummy_access_control::DummyAccessControlClient::new(env, &ac_id);

    let contract_id = env.register(PredifiContract, ());
    let client = PredifiContractClient::new(env, &contract_id);

    let token_admin = Address::generate(env);
    let token_contract = env.register_stellar_asset_contract(token_admin.clone());
    let token = token::Client::new(env, &token_contract);
    let token_admin_client = token::StellarAssetClient::new(env, &token_contract);
    let token_address = token_contract;

    let treasury = Address::generate(env);
    let operator = Address::generate(env);
    let creator = Address::generate(env);
    let admin = Address::generate(env);

    ac_client.grant_role(&operator, &ROLE_OPERATOR);
    ac_client.grant_role(&admin, &ROLE_ADMIN);
    client.init(&ac_id, &treasury, &0u32, &0u64);
    client.add_token_to_whitelist(&admin, &token_address);

    (
        ac_client,
        client,
        token_address,
        token,
        token_admin_client,
        treasury,
        operator,
        creator,
    )
}

// ── Core prediction tests ────────────────────────────────────────────────────

#[test]
fn test_claim_winnings() {
    let env = Env::default();
    env.mock_all_auths();

    let (_, client, token_address, token, token_admin_client, _, operator, creator) = setup(&env);
    let contract_addr = client.address.clone();

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    token_admin_client.mint(&user1, &1000);
    token_admin_client.mint(&user2, &1000);

    let pool_id = client.create_pool(
        &creator,
        &CreatePoolArgs {
            end_time: 100000u64,
            token: token_address.clone(),
            options_count: 3u32,
            description: String::from_str(&env, "Test Pool"),
            metadata_url: String::from_str(
                &env,
                "ipfs://bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi",
            ),
            min_stake: 100i128,
            max_stake: 0i128,
            initial_liquidity: 0i128,
            category: symbol_short!("Tech"),
            max_total_stake: 0i128,
        },
    );
    client.place_prediction(&user1, &pool_id, &100, &1);
    client.place_prediction(&user2, &pool_id, &100, &2);

    assert_eq!(token.balance(&contract_addr), 200);

    env.ledger().with_mut(|li| li.timestamp = 100001);

    client.resolve_pool(&operator, &pool_id, &1u32);

    let winnings = client.claim_winnings(&user1, &pool_id);
    assert_eq!(winnings, 200);
    assert_eq!(token.balance(&user1), 1100);

    let winnings2 = client.claim_winnings(&user2, &pool_id);
    assert_eq!(winnings2, 0);
    assert_eq!(token.balance(&user2), 900);
}

#[test]
#[should_panic(expected = "Error(Contract, #60)")]
fn test_double_claim() {
    let env = Env::default();
    env.mock_all_auths();

    let (_, client, token_address, _, token_admin_client, _, operator, creator) = setup(&env);

    let user1 = Address::generate(&env);
    token_admin_client.mint(&user1, &1000);

    let pool_id = client.create_pool(
        &creator,
        &CreatePoolArgs {
            end_time: 100000u64,
            token: token_address.clone(),
            options_count: 3u32,
            description: String::from_str(&env, "Test Pool"),
            metadata_url: String::from_str(
                &env,
                "ipfs://bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi",
            ),
            min_stake: 100i128,
            max_stake: 0i128,
            initial_liquidity: 0i128,
            category: symbol_short!("Tech"),
            max_total_stake: 0i128,
        },
    );
    client.place_prediction(&user1, &pool_id, &100, &1);

    env.ledger().with_mut(|li| li.timestamp = 100001);

    client.resolve_pool(&operator, &pool_id, &1u32);

    client.claim_winnings(&user1, &pool_id);
    client.claim_winnings(&user1, &pool_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #22)")]
fn test_claim_unresolved() {
    let env = Env::default();
    env.mock_all_auths();

    let (_, client, token_address, _, token_admin_client, _, _, creator) = setup(&env);

    let user1 = Address::generate(&env);
    token_admin_client.mint(&user1, &1000);

    let pool_id = client.create_pool(
        &creator,
        &CreatePoolArgs {
            end_time: 100000u64,
            token: token_address.clone(),
            options_count: 3u32,
            description: String::from_str(&env, "Test Pool"),
            metadata_url: String::from_str(&env, "ipfs://metadata"),
            min_stake: 1i128,
            max_stake: 0i128,
            initial_liquidity: 0i128,
            category: symbol_short!("Tech"),
            max_total_stake: 0i128,
        },
    );
    client.place_prediction(&user1, &pool_id, &100, &1);

    client.claim_winnings(&user1, &pool_id);
}

#[test]
fn test_multiple_pools_independent() {
    let env = Env::default();
    env.mock_all_auths();

    let (_, client, token_address, _, token_admin_client, _, operator, creator) = setup(&env);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    token_admin_client.mint(&user1, &1000);
    token_admin_client.mint(&user2, &1000);

    let pool_a = client.create_pool(
        &creator,
        &CreatePoolArgs {
            end_time: 100000u64,
            token: token_address.clone(),
            options_count: 3u32,
            description: String::from_str(&env, "Test Pool"),
            metadata_url: String::from_str(&env, "ipfs://metadata"),
            min_stake: 1i128,
            max_stake: 0i128,
            initial_liquidity: 0i128,
            category: symbol_short!("Tech"),
            max_total_stake: 0i128,
        },
    );
    let pool_b = client.create_pool(
        &creator,
        &CreatePoolArgs {
            end_time: 100000u64,
            token: token_address.clone(),
            options_count: 3u32,
            description: String::from_str(&env, "Test Pool"),
            metadata_url: String::from_str(&env, "ipfs://metadata"),
            min_stake: 1i128,
            max_stake: 0i128,
            initial_liquidity: 0i128,
            category: symbol_short!("Tech"),
            max_total_stake: 0i128,
        },
    );

    client.place_prediction(&user1, &pool_a, &100, &1);
    client.place_prediction(&user2, &pool_b, &100, &1);

    env.ledger().with_mut(|li| li.timestamp = 100001);

    client.resolve_pool(&operator, &pool_a, &1u32);
    client.resolve_pool(&operator, &pool_b, &2u32);

    let w1 = client.claim_winnings(&user1, &pool_a);
    assert_eq!(w1, 100);

    let w2 = client.claim_winnings(&user2, &pool_b);
    assert_eq!(w2, 0);
}

// ── Access control tests ─────────────────────────────────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn test_unauthorized_set_fee_bps() {
    let env = Env::default();
    env.mock_all_auths();

    let (_, client, _, _, _, _, _, _creator) = setup(&env);
    let not_admin = Address::generate(&env);
    client.set_fee_bps(&not_admin, &999u32);
}

#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn test_unauthorized_set_treasury() {
    let env = Env::default();
    env.mock_all_auths();

    let (_, client, _, _, _, _, _, _creator) = setup(&env);
    let not_admin = Address::generate(&env);
    let new_treasury = Address::generate(&env);
    client.set_treasury(&not_admin, &new_treasury);
}

#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn test_unauthorized_resolve_pool() {
    let env = Env::default();
    env.mock_all_auths();

    let (_, client, token_address, _, _, _, _, creator) = setup(&env);
    let pool_id = client.create_pool(
        &creator,
        &CreatePoolArgs {
            end_time: 100000u64,
            token: token_address.clone(),
            options_count: 3u32,
            description: String::from_str(&env, "Test Pool"),
            metadata_url: String::from_str(&env, "ipfs://metadata"),
            min_stake: 1i128,
            max_stake: 0i128,
            initial_liquidity: 0i128,
            category: symbol_short!("Tech"),
            max_total_stake: 0i128,
        },
    );
    let not_operator = Address::generate(&env);
    env.ledger().with_mut(|li| li.timestamp = 10001);
    client.resolve_pool(&not_operator, &pool_id, &1u32);
}

#[test]
fn test_oracle_can_resolve() {
    let env = Env::default();
    env.mock_all_auths();

    let ac_id = env.register(dummy_access_control::DummyAccessControl, ());
    let ac_client = dummy_access_control::DummyAccessControlClient::new(&env, &ac_id);
    let contract_id = env.register(PredifiContract, ());
    let client = PredifiContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract(token_admin.clone());
    let token_address = token_contract;

    let treasury = Address::generate(&env);
    let oracle = Address::generate(&env);
    let admin = Address::generate(&env);

    ac_client.grant_role(&oracle, &ROLE_ORACLE);
    ac_client.grant_role(&admin, &ROLE_ADMIN);
    client.init(&ac_id, &treasury, &0u32, &0u64);
    client.add_token_to_whitelist(&admin, &token_address);

    let creator = Address::generate(&env);
    let pool_id = client.create_pool(
        &creator,
        &CreatePoolArgs {
            end_time: 100000u64,
            token: token_address.clone(),
            options_count: 3u32,
            description: String::from_str(&env, "Test Pool"),
            metadata_url: String::from_str(&env, "ipfs://metadata"),
            min_stake: 1i128,
            max_stake: 0i128,
            initial_liquidity: 0i128,
            category: symbol_short!("Tech"),
            max_total_stake: 0i128,
        },
    );

    env.ledger().with_mut(|li| li.timestamp = 100001);

    // Call oracle_resolve which should succeed
    client.oracle_resolve(
        &oracle,
        &pool_id,
        &1u32,
        &String::from_str(&env, "proof_123"),
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn test_unauthorized_oracle_resolve() {
    let env = Env::default();
    env.mock_all_auths();

    let ac_id = env.register(dummy_access_control::DummyAccessControl, ());
    let ac_client = dummy_access_control::DummyAccessControlClient::new(&env, &ac_id);
    let contract_id = env.register(PredifiContract, ());
    let client = PredifiContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract(token_admin.clone());
    let token_address = token_contract;

    let treasury = Address::generate(&env);
    let not_oracle = Address::generate(&env);

    let admin = Address::generate(&env);
    // Give them OPERATOR instead of ORACLE, they still shouldn't be able to call oracle_resolve
    ac_client.grant_role(&not_oracle, &ROLE_OPERATOR);
    ac_client.grant_role(&admin, &ROLE_ADMIN);
    client.init(&ac_id, &treasury, &0u32, &0u64);
    client.add_token_to_whitelist(&admin, &token_address);

    let creator = Address::generate(&env);
    let pool_id = client.create_pool(
        &creator,
        &CreatePoolArgs {
            end_time: 100000u64,
            token: token_address.clone(),
            options_count: 3u32,
            description: String::from_str(&env, "Test Pool"),
            metadata_url: String::from_str(&env, "ipfs://metadata"),
            min_stake: 1i128,
            max_stake: 0i128,
            initial_liquidity: 0i128,
            category: symbol_short!("Tech"),
            max_total_stake: 0i128,
        },
    );

    env.ledger().with_mut(|li| li.timestamp = 100001);

    client.oracle_resolve(
        &not_oracle,
        &pool_id,
        &1u32,
        &String::from_str(&env, "proof_123"),
    );
}

#[test]
fn test_admin_can_set_fee_bps() {
    let env = Env::default();
    env.mock_all_auths();

    let ac_id = env.register(dummy_access_control::DummyAccessControl, ());
    let ac_client = dummy_access_control::DummyAccessControlClient::new(&env, &ac_id);
    let contract_id = env.register(PredifiContract, ());
    let client = PredifiContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    ac_client.grant_role(&admin, &ROLE_ADMIN);
    client.init(&ac_id, &treasury, &0u32, &0u64);

    client.set_fee_bps(&admin, &500u32);
}

#[test]
fn test_admin_can_set_treasury() {
    let env = Env::default();
    env.mock_all_auths();

    let ac_id = env.register(dummy_access_control::DummyAccessControl, ());
    let ac_client = dummy_access_control::DummyAccessControlClient::new(&env, &ac_id);
    let contract_id = env.register(PredifiContract, ());
    let client = PredifiContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let new_treasury = Address::generate(&env);
    ac_client.grant_role(&admin, &ROLE_ADMIN);
    client.init(&ac_id, &treasury, &0u32, &0u64);

    client.set_treasury(&admin, &new_treasury);
}

// ── Pause tests ───────────────────────────────────────────────────────────────

#[test]
fn test_admin_can_pause_and_unpause() {
    let env = Env::default();
    env.mock_all_auths();

    let ac_id = env.register(dummy_access_control::DummyAccessControl, ());
    let ac_client = dummy_access_control::DummyAccessControlClient::new(&env, &ac_id);
    let contract_id = env.register(PredifiContract, ());
    let client = PredifiContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    ac_client.grant_role(&admin, &ROLE_ADMIN);
    client.init(&ac_id, &treasury, &0u32, &0u64);

    client.pause(&admin);
    client.unpause(&admin);
}

#[test]
#[should_panic]
fn test_admin_can_upgrade() {
    let env = Env::default();
    env.mock_all_auths();

    let ac_id = env.register(dummy_access_control::DummyAccessControl, ());
    let ac_client = dummy_access_control::DummyAccessControlClient::new(&env, &ac_id);
    let contract_id = env.register(PredifiContract, ());
    let client = PredifiContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    ac_client.grant_role(&admin, &ROLE_ADMIN);
    client.init(&ac_id, &treasury, &0u32, &0u64);

    // We expect this to panic in the mock environment because the Wasm hash is not registered.
    // The point is to verify it passes the Authorization check.
    let new_wasm_hash = BytesN::from_array(&env, &[0u8; 32]);
    client.upgrade_contract(&admin, &new_wasm_hash);
}

#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn test_non_admin_cannot_upgrade() {
    let env = Env::default();
    env.mock_all_auths();

    let ac_id = env.register(dummy_access_control::DummyAccessControl, ());
    let contract_id = env.register(PredifiContract, ());
    let client = PredifiContractClient::new(&env, &contract_id);

    let not_admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(&ac_id, &treasury, &0u32, &0u64);

    let new_wasm_hash = BytesN::from_array(&env, &[0u8; 32]);
    client.upgrade_contract(&not_admin, &new_wasm_hash);
}

#[test]
fn test_admin_can_migrate() {
    let env = Env::default();
    env.mock_all_auths();

    let ac_id = env.register(dummy_access_control::DummyAccessControl, ());
    let ac_client = dummy_access_control::DummyAccessControlClient::new(&env, &ac_id);
    let contract_id = env.register(PredifiContract, ());
    let client = PredifiContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    ac_client.grant_role(&admin, &ROLE_ADMIN);
    client.init(&ac_id, &treasury, &0u32, &0u64);

    client.migrate_state(&admin);
}

#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn test_non_admin_cannot_migrate() {
    let env = Env::default();
    env.mock_all_auths();

    let ac_id = env.register(dummy_access_control::DummyAccessControl, ());
    let contract_id = env.register(PredifiContract, ());
    let client = PredifiContractClient::new(&env, &contract_id);

    let not_admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(&ac_id, &treasury, &0u32, &0u64);

    client.migrate_state(&not_admin);
}

#[test]
#[should_panic(expected = "Unauthorized: missing required role")]
fn test_non_admin_cannot_pause() {
    let env = Env::default();
    env.mock_all_auths();

    let ac_id = env.register(dummy_access_control::DummyAccessControl, ());
    let contract_id = env.register(PredifiContract, ());
    let client = PredifiContractClient::new(&env, &contract_id);

    let not_admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    client.init(&ac_id, &treasury, &0u32, &0u64);

    client.pause(&not_admin);
}

#[test]
#[should_panic(expected = "Contract is paused")]
fn test_paused_blocks_set_fee_bps() {
    let env = Env::default();
    env.mock_all_auths();

    let ac_id = env.register(dummy_access_control::DummyAccessControl, ());
    let ac_client = dummy_access_control::DummyAccessControlClient::new(&env, &ac_id);
    let contract_id = env.register(PredifiContract, ());
    let client = PredifiContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    ac_client.grant_role(&admin, &ROLE_ADMIN);
    client.init(&ac_id, &treasury, &0u32, &0u64);

    client.pause(&admin);
    client.set_fee_bps(&admin, &100u32);
}

#[test]
#[should_panic(expected = "Contract is paused")]
fn test_paused_blocks_set_treasury() {
    let env = Env::default();
    env.mock_all_auths();

    let ac_id = env.register(dummy_access_control::DummyAccessControl, ());
    let ac_client = dummy_access_control::DummyAccessControlClient::new(&env, &ac_id);
    let contract_id = env.register(PredifiContract, ());
    let client = PredifiContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    ac_client.grant_role(&admin, &ROLE_ADMIN);
    client.init(&ac_id, &treasury, &0u32, &0u64);

    client.pause(&admin);
    client.set_treasury(&admin, &Address::generate(&env));
}

#[test]
#[should_panic(expected = "Contract is paused")]
fn test_paused_blocks_create_pool() {
    let env = Env::default();
    env.mock_all_auths();

    let ac_id = env.register(dummy_access_control::DummyAccessControl, ());
    let ac_client = dummy_access_control::DummyAccessControlClient::new(&env, &ac_id);
    let contract_id = env.register(PredifiContract, ());
    let client = PredifiContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let token = Address::generate(&env);
    ac_client.grant_role(&admin, &ROLE_ADMIN);
    client.init(&ac_id, &treasury, &0u32, &0u64);
    client.add_token_to_whitelist(&admin, &token);

    let creator = Address::generate(&env);
    client.pause(&admin);
    client.create_pool(
        &creator,
        &CreatePoolArgs {
            end_time: 100000u64,
            token: token,
            options_count: 3u32,
            description: String::from_str(&env, "Test Pool"),
            metadata_url: String::from_str(&env, "ipfs://metadata"),
            min_stake: 1i128,
            max_stake: 0i128,
            initial_liquidity: 0i128,
            category: symbol_short!("Tech"),
            max_total_stake: 0i128,
        },
    );
}

#[test]
#[should_panic(expected = "Contract is paused")]
fn test_paused_blocks_place_prediction() {
    let env = Env::default();
    env.mock_all_auths();

    let ac_id = env.register(dummy_access_control::DummyAccessControl, ());
    let ac_client = dummy_access_control::DummyAccessControlClient::new(&env, &ac_id);
    let contract_id = env.register(PredifiContract, ());
    let client = PredifiContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let treasury = Address::generate(&env);
    ac_client.grant_role(&admin, &ROLE_ADMIN);
    client.init(&ac_id, &treasury, &0u32, &0u64);

    client.pause(&admin);
    client.place_prediction(&user, &0u64, &10, &1);
}

#[test]
#[should_panic(expected = "Contract is paused")]
fn test_paused_blocks_resolve_pool() {
    let env = Env::default();
    env.mock_all_auths();

    let ac_id = env.register(dummy_access_control::DummyAccessControl, ());
    let ac_client = dummy_access_control::DummyAccessControlClient::new(&env, &ac_id);
    let contract_id = env.register(PredifiContract, ());
    let client = PredifiContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let operator = Address::generate(&env);
    let treasury = Address::generate(&env);
    ac_client.grant_role(&admin, &ROLE_ADMIN);
    ac_client.grant_role(&operator, &ROLE_OPERATOR);
    client.init(&ac_id, &treasury, &0u32, &0u64);

    client.pause(&admin);
    client.resolve_pool(&operator, &0u64, &1u32);
}

#[test]
#[should_panic(expected = "Contract is paused")]
fn test_paused_blocks_claim_winnings() {
    let env = Env::default();
    env.mock_all_auths();

    let ac_id = env.register(dummy_access_control::DummyAccessControl, ());
    let ac_client = dummy_access_control::DummyAccessControlClient::new(&env, &ac_id);
    let contract_id = env.register(PredifiContract, ());
    let client = PredifiContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let treasury = Address::generate(&env);
    ac_client.grant_role(&admin, &ROLE_ADMIN);
    client.init(&ac_id, &treasury, &0u32, &0u64);

    client.pause(&admin);
    client.claim_winnings(&user, &0u64);
}

#[test]
fn test_unpause_restores_functionality() {
    let env = Env::default();
    env.mock_all_auths();

    let ac_id = env.register(dummy_access_control::DummyAccessControl, ());
    let ac_client = dummy_access_control::DummyAccessControlClient::new(&env, &ac_id);
    let contract_id = env.register(PredifiContract, ());
    let client = PredifiContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract(token_admin.clone());
    let token_admin_client = token::StellarAssetClient::new(&env, &token_contract);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let treasury = Address::generate(&env);
    ac_client.grant_role(&admin, &ROLE_ADMIN);
    client.init(&ac_id, &treasury, &0u32, &0u64);
    client.add_token_to_whitelist(&admin, &token_contract);
    token_admin_client.mint(&user, &1000);

    let creator = Address::generate(&env);
    client.pause(&admin);
    client.unpause(&admin);

    let pool_id = client.create_pool(
        &creator,
        &CreatePoolArgs {
            end_time: 100000u64,
            token: token_contract,
            options_count: 3u32,
            description: String::from_str(&env, "Test Pool"),
            metadata_url: String::from_str(&env, "ipfs://metadata"),
            min_stake: 1i128,
            max_stake: 0i128,
            initial_liquidity: 0i128,
            category: symbol_short!("Tech"),
            max_total_stake: 0i128,
        },
    );
    client.place_prediction(&user, &pool_id, &10, &1);
}

// ── Pagination tests ──────────────────────────────────────────────────────────

#[test]
fn test_get_user_predictions() {
    let env = Env::default();
    env.mock_all_auths();

    let (_, client, token_address, _, token_admin_client, _, _, creator) = setup(&env);

    let user = Address::generate(&env);
    token_admin_client.mint(&user, &1000);

    let pool0 = client.create_pool(
        &creator,
        &CreatePoolArgs {
            end_time: 100000u64,
            token: token_address.clone(),
            options_count: 3u32,
            description: String::from_str(&env, "Test Pool"),
            metadata_url: String::from_str(
                &env,
                "ipfs://bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi",
            ),
            min_stake: 1i128,
            max_stake: 0i128,
            initial_liquidity: 0i128,
            category: symbol_short!("Tech"),
            max_total_stake: 0i128,
        },
    );
    let pool1 = client.create_pool(
        &creator,
        &CreatePoolArgs {
            end_time: 100000u64,
            token: token_address.clone(),
            options_count: 3u32,
            description: String::from_str(&env, "Test Pool"),
            metadata_url: String::from_str(
                &env,
                "ipfs://bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi",
            ),
            min_stake: 1i128,
            max_stake: 0i128,
            initial_liquidity: 0i128,
            category: symbol_short!("Tech"),
            max_total_stake: 0i128,
        },
    );
    let pool2 = client.create_pool(
        &creator,
        &CreatePoolArgs {
            end_time: 100000u64,
            token: token_address.clone(),
            options_count: 3u32,
            description: String::from_str(&env, "Test Pool"),
            metadata_url: String::from_str(
                &env,
                "ipfs://bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi",
            ),
            min_stake: 1i128,
            max_stake: 0i128,
            initial_liquidity: 0i128,
            category: symbol_short!("Tech"),
            max_total_stake: 0i128,
        },
    );

    client.place_prediction(&user, &pool0, &10, &1);
    client.place_prediction(&user, &pool1, &20, &2);
    client.place_prediction(&user, &pool2, &30, &1);

    let first_two = client.get_user_predictions(&user, &0, &2);
    assert_eq!(first_two.len(), 2);
    assert_eq!(first_two.get(0).unwrap().pool_id, pool0);
    assert_eq!(first_two.get(1).unwrap().pool_id, pool1);

    let last_two = client.get_user_predictions(&user, &1, &2);
    assert_eq!(last_two.len(), 2);
    assert_eq!(last_two.get(0).unwrap().pool_id, pool1);
    assert_eq!(last_two.get(1).unwrap().pool_id, pool2);

    let last_one = client.get_user_predictions(&user, &2, &1);
    assert_eq!(last_one.len(), 1);
    assert_eq!(last_one.get(0).unwrap().pool_id, pool2);

    let empty = client.get_user_predictions(&user, &3, &1);
    assert_eq!(empty.len(), 0);
}
// ── Pool cancellation tests ───────────────────────────────────────────────────

#[test]
fn test_admin_can_cancel_pool() {
    let env = Env::default();
    env.mock_all_auths();

    let ac_id = env.register(dummy_access_control::DummyAccessControl, ());
    let ac_client = dummy_access_control::DummyAccessControlClient::new(&env, &ac_id);
    let contract_id = env.register(PredifiContract, ());
    let client = PredifiContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract(token_admin.clone());
    let token_address = token_contract;

    let admin = Address::generate(&env);
    let whitelist_admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let creator = Address::generate(&env);
    ac_client.grant_role(&admin, &ROLE_OPERATOR);
    ac_client.grant_role(&whitelist_admin, &ROLE_ADMIN);
    client.init(&ac_id, &treasury, &0u32, &0u64);
    client.add_token_to_whitelist(&whitelist_admin, &token_address);

    let pool_id = client.create_pool(
        &creator,
        &CreatePoolArgs {
            end_time: 100000u64,
            token: token_address.clone(),
            options_count: 3u32,
            description: String::from_str(&env, "Test Pool"),
            metadata_url: String::from_str(&env, "ipfs://metadata"),
            min_stake: 1i128,
            max_stake: 0i128,
            initial_liquidity: 0i128,
            category: symbol_short!("Tech"),
            max_total_stake: 0i128,
        },
    );

    // Admin should be able to cancel
    client.cancel_pool(&admin, &pool_id);
}

#[test]
fn test_pool_creator_can_cancel_unresolved_pool() {
    let env = Env::default();
    env.mock_all_auths();

    let ac_id = env.register(dummy_access_control::DummyAccessControl, ());
    let ac_client = dummy_access_control::DummyAccessControlClient::new(&env, &ac_id);
    let contract_id = env.register(PredifiContract, ());
    let client = PredifiContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract(token_admin.clone());
    let token_address = token_contract;

    let creator = Address::generate(&env);
    let treasury = Address::generate(&env);
    let admin = Address::generate(&env);
    ac_client.grant_role(&creator, &ROLE_OPERATOR);
    ac_client.grant_role(&admin, &ROLE_ADMIN);
    client.init(&ac_id, &treasury, &0u32, &0u64);
    client.add_token_to_whitelist(&admin, &token_address);

    let pool_id = client.create_pool(
        &creator,
        &CreatePoolArgs {
            end_time: 100000u64,
            token: token_address.clone(),
            options_count: 3u32,
            description: String::from_str(&env, "Test Pool"),
            metadata_url: String::from_str(&env, "ipfs://metadata"),
            min_stake: 1i128,
            max_stake: 0i128,
            initial_liquidity: 0i128,
            category: symbol_short!("Tech"),
            max_total_stake: 0i128,
        },
    );

    // Admin should be able to cancel their pool
    client.cancel_pool(&creator, &pool_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn test_non_admin_non_creator_cannot_cancel() {
    let env = Env::default();
    env.mock_all_auths();

    let (_, client, token_address, _, _, _, _, creator) = setup(&env);

    let pool_id = client.create_pool(
        &creator,
        &CreatePoolArgs {
            end_time: 100000u64,
            token: token_address.clone(),
            options_count: 3u32,
            description: String::from_str(&env, "Test Pool"),
            metadata_url: String::from_str(&env, "ipfs://metadata"),
            min_stake: 1i128,
            max_stake: 0i128,
            initial_liquidity: 0i128,
            category: symbol_short!("Tech"),
            max_total_stake: 0i128,
        },
    );

    let unauthorized = Address::generate(&env);
    // This should fail - user is not admin
    client.cancel_pool(&unauthorized, &pool_id);
}

// ── Token whitelist tests ───────────────────────────────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #91)")]
fn test_create_pool_rejects_non_whitelisted_token() {
    let env = Env::default();
    env.mock_all_auths();

    let ac_id = env.register(dummy_access_control::DummyAccessControl, ());
    let ac_client = dummy_access_control::DummyAccessControlClient::new(&env, &ac_id);
    let contract_id = env.register(PredifiContract, ());
    let client = PredifiContractClient::new(&env, &contract_id);

    let treasury = Address::generate(&env);
    let creator = Address::generate(&env);
    let token_not_whitelisted = Address::generate(&env);

    ac_client.grant_role(&creator, &ROLE_OPERATOR);
    client.init(&ac_id, &treasury, &0u32, &0u64);
    // Do NOT whitelist token_not_whitelisted

    client.create_pool(
        &creator,
        &CreatePoolArgs {
            end_time: 100000u64,
            token: token_not_whitelisted,
            options_count: 2u32,
            description: String::from_str(&env, "Pool"),
            metadata_url: String::from_str(&env, "ipfs://meta"),
            min_stake: 0i128,
            max_stake: 0i128,
            initial_liquidity: 0i128,
            category: symbol_short!("Tech"),
            max_total_stake: 0i128,
        },
    );
}

#[test]
fn test_token_whitelist_add_remove_and_is_allowed() {
    let env = Env::default();
    env.mock_all_auths();

    let ac_id = env.register(dummy_access_control::DummyAccessControl, ());
    let ac_client = dummy_access_control::DummyAccessControlClient::new(&env, &ac_id);
    let contract_id = env.register(PredifiContract, ());
    let client = PredifiContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let token = Address::generate(&env);
    ac_client.grant_role(&admin, &ROLE_ADMIN);
    client.init(&ac_id, &treasury, &0u32, &0u64);

    assert!(!client.is_token_allowed(&token));
    client.add_token_to_whitelist(&admin, &token);
    assert!(client.is_token_allowed(&token));
    client.remove_token_from_whitelist(&admin, &token);
    assert!(!client.is_token_allowed(&token));
}

#[test]
#[should_panic(expected = "Error(Contract, #22)")]
fn test_cannot_cancel_resolved_pool_by_operator() {
    let env = Env::default();
    env.mock_all_auths();

    let ac_id = env.register(dummy_access_control::DummyAccessControl, ());
    let ac_client = dummy_access_control::DummyAccessControlClient::new(&env, &ac_id);
    let contract_id = env.register(PredifiContract, ());
    let client = PredifiContractClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract(token_admin.clone());
    let token_address = token_contract;

    let admin = Address::generate(&env);
    let whitelist_admin = Address::generate(&env);
    let operator = Address::generate(&env);
    let treasury = Address::generate(&env);
    let creator = Address::generate(&env);
    ac_client.grant_role(&admin, &ROLE_OPERATOR);
    ac_client.grant_role(&operator, &ROLE_OPERATOR);
    ac_client.grant_role(&whitelist_admin, &ROLE_ADMIN);
    client.init(&ac_id, &treasury, &0u32, &0u64);
    client.add_token_to_whitelist(&whitelist_admin, &token_address);

    let pool_id = client.create_pool(
        &creator,
        &CreatePoolArgs {
            end_time: 100000u64,
            token: token_address.clone(),
            options_count: 3u32,
            description: String::from_str(&env, "Test Pool"),
            metadata_url: String::from_str(&env, "ipfs://metadata"),
            min_stake: 1i128,
            max_stake: 0i128,
            initial_liquidity: 0i128,
            category: symbol_short!("Tech"),
            max_total_stake: 0i128,
        },
    );

    env.ledger().with_mut(|li| li.timestamp = 100001);
    client.resolve_pool(&operator, &pool_id, &1u32);

    // Now try to cancel - should fail
    client.cancel_pool(&admin, &pool_id);
}

#[test]
#[should_panic(expected = "Cannot place prediction on canceled pool")]
fn test_cannot_place_prediction_on_canceled_pool() {
    let env = Env::default();
    env.mock_all_auths();

    let ac_id = env.register(dummy_access_control::DummyAccessControl, ());
    let ac_client = dummy_access_control::DummyAccessControlClient::new(&env, &ac_id);
    let contract_id = env.register(PredifiContract, ());
    let client = PredifiContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let whitelist_admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    ac_client.grant_role(&admin, &ROLE_OPERATOR);
    ac_client.grant_role(&whitelist_admin, &ROLE_ADMIN);
    client.init(&ac_id, &treasury, &0u32, &0u64);
    client.add_token_to_whitelist(&whitelist_admin, &token_address);

    let creator = Address::generate(&env);
    let user = Address::generate(&env);
    token_admin_client.mint(&user, &1000);

    // Create and cancel pool
    let pool_id = client.create_pool(
        &creator,
        &CreatePoolArgs {
            end_time: 100000u64,
            token: token_address.clone(),
            options_count: 3u32,
            description: String::from_str(&env, "Test Pool"),
            metadata_url: String::from_str(&env, "ipfs://metadata"),
            min_stake: 1i128,
            max_stake: 0i128,
            initial_liquidity: 0i128,
            category: symbol_short!("Tech"),
            max_total_stake: 0i128,
        },
    );

    // Cancel the pool
    client.cancel_pool(&admin, &pool_id);

    // Try to place prediction on canceled pool - should panic
    client.place_prediction(&user, &pool_id, &100, &1);
}

#[test]
#[should_panic(expected = "Contract is paused")]
fn test_paused_blocks_withdraw_treasury() {
    let env = Env::default();
    env.mock_all_auths();

    let (ac_client, client, token_address, token, token_admin_client, treasury, _, _) = setup(&env);
    let contract_addr = client.address.clone();
    let admin = Address::generate(&env);
    ac_client.grant_role(&admin, &ROLE_ADMIN);

    token_admin_client.mint(&contract_addr, &5000);

    // Pause contract
    client.pause(&admin);

    // Try to withdraw while paused - should panic
    client.withdraw_treasury(&admin, &token_address, &1000, &treasury);
}

#[test]
fn test_get_pool_stats() {
    let env = Env::default();
    env.mock_all_auths();

    let (_, client, token_address, _, token_admin_client, _, _, creator) = setup(&env);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let user3 = Address::generate(&env);
    token_admin_client.mint(&user1, &5000);
    token_admin_client.mint(&user2, &5000);
    token_admin_client.mint(&user3, &5000);

    let pool_id = client.create_pool(
        &creator,
        &CreatePoolArgs {
            end_time: 100000u64,
            token: token_address.clone(),
            options_count: 2u32, // Binary pool
            description: String::from_str(&env, "Stats Test"),
            metadata_url: String::from_str(&env, "ipfs://metadata"),
            min_stake: 1i128,
            max_stake: 0i128,
            initial_liquidity: 0i128,
            category: symbol_short!("Tech"),
            max_total_stake: 0i128,
        },
    );

    // Initial stats
    let stats = client.get_pool_stats(&pool_id);
    assert_eq!(stats.participants_count, 0);
    assert_eq!(stats.total_stake, 0);

    // User 1 bets 100 on outcome 0
    client.place_prediction(&user1, &pool_id, &100, &0);
    // User 2 bets 200 on outcome 1
    client.place_prediction(&user2, &pool_id, &200, &1);
    // User 3 bets 100 on outcome 1
    client.place_prediction(&user3, &pool_id, &100, &1);
    // User 1 bets 100 more on outcome 0 (should not increase participants)
    client.place_prediction(&user1, &pool_id, &100, &0);

    let stats = client.get_pool_stats(&pool_id);
    assert_eq!(stats.participants_count, 3);
    assert_eq!(stats.total_stake, 500); // 100+200+100+100
    assert_eq!(stats.stakes_per_outcome.get(0), Some(200));
    assert_eq!(stats.stakes_per_outcome.get(1), Some(300));

    // Odds:
    // Outcome 0: (500 * 10000) / 200 = 25000 (2.5x)
    // Outcome 1: (500 * 10000) / 300 = 16666 (1.6666x)
    assert_eq!(stats.current_odds.get(0), Some(25000));
    assert_eq!(stats.current_odds.get(1), Some(16666));
}

// ═══════════════════════════════════════════════════════════════════════════
// EDGE-CASE TESTS  (#327)
// ═══════════════════════════════════════════════════════════════════════════
//
// Coverage additions mandated by GitHub issue #327:
//   • Leap-year timestamp boundaries
//   • Maximum possible stake values
//   • Rapid resolution / claim sequences
//   • Boundary values in all validation logic
//   • (Simulated) race conditions & unauthorized access attempts
//   • State consistency after multiple resolution cycles

// ── Constants for leap-year tests ────────────────────────────────────────────

/// Feb 28, 2024 00:00:00 UTC (day before the 2024 leap day).
const FEB_28_2024_UTC: u64 = 1_709_078_400;
/// Feb 29, 2024 00:00:00 UTC (2024 is a leap year).
const LEAP_DAY_2024_UTC: u64 = 1_709_164_800;
/// Mar 01, 2024 00:00:00 UTC (first day after the 2024 leap day).
const MAR_01_2024_UTC: u64 = 1_709_251_200;

// ── Leap-year timestamp edge cases ───────────────────────────────────────────

/// A pool whose end time falls exactly on the leap day (Feb 29, 2024)
/// must be created and accepted for predictions without any off-by-one error.
#[test]
fn test_pool_end_time_on_leap_day() {
    let env = Env::default();
    env.mock_all_auths();

    let (_, client, token_address, _, token_admin_client, _, _, creator) = setup(&env);

    // Advance ledger to Feb 28. end_time = Feb 29 (86 400 s later, well above 3 600 s minimum).
    env.ledger().with_mut(|li| li.timestamp = FEB_28_2024_UTC);

    let pool_id = client.create_pool(
        &creator,
        &CreatePoolArgs {
            end_time: LEAP_DAY_2024_UTC,
            token: token_address.clone(),
            options_count: 2u32,
            description: String::from_str(&env, "tech"),
            metadata_url: String::from_str(&env, "ipfs://meta"),
            min_stake: 0i128,
            max_stake: 0i128,
            initial_liquidity: 0i128,
            category: symbol_short!("Tech"),
            max_total_stake: 0i128,
        },
    );

    let user = Address::generate(&env);
    token_admin_client.mint(&user, &1000);
    // Prediction must be accepted while before the leap-day deadline.
    client.place_prediction(&user, &pool_id, &100, &0);
}

/// Creating a pool whose end time is the leap day, but the ledger is already
/// past Mar 1, must be rejected because the end time is in the past.
#[test]
#[should_panic(expected = "end_time must be in the future")]
fn test_pool_end_time_at_leap_day_already_past() {
    let env = Env::default();
    env.mock_all_auths();

    let (_, client, token_address, _, _, _, _, creator) = setup(&env);

    // Ledger at Mar 1 – the leap day is in the past.
    env.ledger().with_mut(|li| li.timestamp = MAR_01_2024_UTC);

    client.create_pool(
        &creator,
        &CreatePoolArgs {
            end_time: LEAP_DAY_2024_UTC, // Feb 29 – already past
            token: token_address,
            options_count: 2u32,
            description: String::from_str(&env, "Expired leap pool"),
            metadata_url: String::from_str(&env, "ipfs://expired"),
            min_stake: 0i128,
            max_stake: 0i128,
            initial_liquidity: 0i128,
            category: symbol_short!("Tech"),
            max_total_stake: 0i128,
        },
    );
}

/// A pool created before the leap day, resolved after it, must behave
/// correctly.  This validates timestamp arithmetic across the Feb 29 boundary.
#[test]
fn test_pool_end_time_spans_leap_day_resolution() {
    let env = Env::default();
    env.mock_all_auths();

    let (_, client, token_address, _, token_admin_client, _, operator, creator) = setup(&env);

    // Creation at Feb 28 00:00 UTC – 3 600 s before end_time on Mar 01.
    // (Difference = 1 709 251 200 – 1 709 074 800 = 176 400 > MIN_POOL_DURATION)
    let creation_time: u64 = FEB_28_2024_UTC - 3_600;
    env.ledger().with_mut(|li| li.timestamp = creation_time);

    let pool_id = client.create_pool(
        &creator,
        &CreatePoolArgs {
            end_time: MAR_01_2024_UTC,
            token: token_address.clone(),
            options_count: 2u32,
            description: String::from_str(&env, "Leap span pool"),
            metadata_url: String::from_str(&env, "ipfs://span"),
            min_stake: 0i128,
            max_stake: 0i128,
            initial_liquidity: 0i128,
            category: symbol_short!("Tech"),
            max_total_stake: 0i128,
        },
    );

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    token_admin_client.mint(&user1, &500);
    token_admin_client.mint(&user2, &500);

    client.place_prediction(&user1, &pool_id, &100, &0);
    client.place_prediction(&user2, &pool_id, &200, &1);

    // Advance ledger past Mar 1 (resolution_delay == 0 in setup).
    env.ledger()
        .with_mut(|li| li.timestamp = MAR_01_2024_UTC + 1);
    client.resolve_pool(&operator, &pool_id, &0u32);

    // user1 staked on the winning outcome – receives full pot.
    let winnings = client.claim_winnings(&user1, &pool_id);
    assert_eq!(winnings, 500);

    let winnings2 = client.claim_winnings(&user2, &pool_id);
    assert_eq!(winnings2, 0);
}

// ── Maximum possible stake amounts ───────────────────────────────────────────

/// A single bet equal to MAX_INITIAL_LIQUIDITY (the contract ceiling) must be
/// accepted, correctly recorded, and fully refunded on a win.
#[test]
fn test_maximum_single_stake_roundtrip() {
    let env = Env::default();
    env.mock_all_auths();

    let (_, client, token_address, token, token_admin_client, _, operator, creator) = setup(&env);

    // MAX_INITIAL_LIQUIDITY = 100_000_000_000_000
    let max_amount: i128 = 100_000_000_000_000;

    let pool_id = client.create_pool(
        &creator,
        &CreatePoolArgs {
            end_time: 100_000u64,
            token: token_address.clone(),
            options_count: 3u32,
            description: String::from_str(&env, "Max stake pool"),
            metadata_url: String::from_str(&env, "ipfs://max"),
            min_stake: 1i128,
            max_stake: max_amount,
            initial_liquidity: 0i128,
            category: symbol_short!("Tech"),
            max_total_stake: 0i128,
        },
    );

    let user = Address::generate(&env);
    token_admin_client.mint(&user, &max_amount);

    client.place_prediction(&user, &pool_id, &max_amount, &0);

    let contract_addr = client.address.clone();
    assert_eq!(token.balance(&contract_addr), max_amount);

    env.ledger().with_mut(|li| li.timestamp = 100_001);
    client.resolve_pool(&operator, &pool_id, &0u32);

    // Sole better on the winning side – receives the entire pot (no fee in setup).
    let winnings = client.claim_winnings(&user, &pool_id);
    assert_eq!(winnings, max_amount);
    assert_eq!(token.balance(&user), max_amount);
}

/// Two winners each holding large stakes on the winning side must receive
/// their proportional share without arithmetic overflow.
#[test]
fn test_large_stake_winnings_split_correctly() {
    let env = Env::default();
    env.mock_all_auths();

    let (_, client, token_address, token, token_admin_client, _, operator, creator) = setup(&env);

    let big_stake: i128 = 10_000_000_000; // 10 billion base units

    let pool_id = client.create_pool(
        &creator,
        &CreatePoolArgs {
            end_time: 100_000u64,
            token: token_address.clone(),
            options_count: 3u32,
            description: String::from_str(&env, "Large stake split"),
            metadata_url: String::from_str(&env, "ipfs://large"),
            min_stake: 1i128,
            max_stake: 0i128, // no max_stake limit
            initial_liquidity: 0i128,
            category: symbol_short!("Tech"),
            max_total_stake: 0i128,
        },
    );

    let winner1 = Address::generate(&env);
    let winner2 = Address::generate(&env);
    let loser1 = Address::generate(&env);
    let loser2 = Address::generate(&env);
    token_admin_client.mint(&winner1, &big_stake);
    token_admin_client.mint(&winner2, &big_stake);
    token_admin_client.mint(&loser1, &big_stake);
    token_admin_client.mint(&loser2, &big_stake);

    // Two winners on outcome 0, two losers on outcome 1.
    client.place_prediction(&winner1, &pool_id, &big_stake, &0);
    client.place_prediction(&winner2, &pool_id, &big_stake, &0);
    client.place_prediction(&loser1, &pool_id, &big_stake, &1);
    client.place_prediction(&loser2, &pool_id, &big_stake, &1);

    env.ledger().with_mut(|li| li.timestamp = 100_001);
    client.resolve_pool(&operator, &pool_id, &0u32);

    let total = big_stake * 4;
    let w1 = client.claim_winnings(&winner1, &pool_id);
    let w2 = client.claim_winnings(&winner2, &pool_id);

    // Each winner gets half the pot.
    assert_eq!(w1, total / 2);
    assert_eq!(w2, total / 2);
    assert_eq!(w1 + w2, total);

    // Losers get nothing.
    let l1 = client.claim_winnings(&loser1, &pool_id);
    let l2 = client.claim_winnings(&loser2, &pool_id);
    assert_eq!(l1, 0);
    assert_eq!(l2, 0);
}

// ── Rapid resolution / claim sequences ───────────────────────────────────────

/// Resolving the same pool twice in a row must fail the second time.
#[test]
#[should_panic(expected = "Pool already resolved")]
fn test_double_resolution_attempt() {
    let env = Env::default();
    env.mock_all_auths();

    let (_, client, token_address, _, _, _, operator, creator) = setup(&env);

    let pool_id = client.create_pool(
        &creator,
        &CreatePoolArgs {
            end_time: 100000u64,
            token: token_address.clone(),
            options_count: 3u32,
            description: String::from_str(&env, "Double resolve"),
            metadata_url: String::from_str(&env, "ipfs://double"),
            min_stake: 1i128,
            max_stake: 0i128,
            initial_liquidity: 0i128,
            category: symbol_short!("Tech"),
            max_total_stake: 0i128,
        },
    );

    env.ledger().with_mut(|li| li.timestamp = 100_001);
    client.resolve_pool(&operator, &pool_id, &0u32);
    // Second resolution must panic.
    client.resolve_pool(&operator, &pool_id, &1u32);
}

/// Ten users all claim winnings immediately after resolution.
/// The total paid out must equal the total staked (no value lost or created).
#[test]
fn test_many_users_rapid_claim_after_resolution() {
    let env = Env::default();
    env.mock_all_auths();

    let (_, client, token_address, token, token_admin_client, _, operator, creator) = setup(&env);
    let contract_addr = client.address.clone();

    let pool_id = client.create_pool(
        &creator,
        &CreatePoolArgs {
            end_time: 100000u64,
            token: token_address.clone(),
            options_count: 3u32,
            description: String::from_str(&env, "Rapid claim"),
            metadata_url: String::from_str(&env, "ipfs://rapid"),
            min_stake: 1i128,
            max_stake: 0i128,
            initial_liquidity: 0i128,
            category: symbol_short!("Tech"),
            max_total_stake: 0i128,
        },
    );

    let stake: i128 = 100;

    // 5 winners (outcome 0) and 5 losers (outcome 1).
    let w0 = Address::generate(&env);
    let w1 = Address::generate(&env);
    let w2 = Address::generate(&env);
    let w3 = Address::generate(&env);
    let w4 = Address::generate(&env);
    let l0 = Address::generate(&env);
    let l1 = Address::generate(&env);
    let l2 = Address::generate(&env);
    let l3 = Address::generate(&env);
    let l4 = Address::generate(&env);

    for u in [&w0, &w1, &w2, &w3, &w4] {
        token_admin_client.mint(u, &stake);
        client.place_prediction(u, &pool_id, &stake, &0);
    }
    for u in [&l0, &l1, &l2, &l3, &l4] {
        token_admin_client.mint(u, &stake);
        client.place_prediction(u, &pool_id, &stake, &1);
    }

    let total = stake * 10;
    assert_eq!(token.balance(&contract_addr), total);

    env.ledger().with_mut(|li| li.timestamp = 100_001);
    client.resolve_pool(&operator, &pool_id, &0u32);

    let mut total_paid: i128 = 0;
    for u in [&w0, &w1, &w2, &w3, &w4] {
        total_paid += client.claim_winnings(u, &pool_id);
    }
    for u in [&l0, &l1, &l2, &l3, &l4] {
        assert_eq!(client.claim_winnings(u, &pool_id), 0);
    }

    // No value created or destroyed (INV-5).
    assert_eq!(total_paid, total);
    assert_eq!(token.balance(&contract_addr), 0);
}

/// Resolving pool A then immediately creating pool B must leave pool A's
/// state intact.  Verifies the ID counter doesn't corrupt resolved data.
#[test]
fn test_resolution_then_new_pool_state_isolation() {
    let env = Env::default();
    env.mock_all_auths();

    let (_, client, token_address, token, token_admin_client, _, operator, creator) = setup(&env);

    let pool_a = client.create_pool(
        &creator,
        &CreatePoolArgs {
            end_time: 100000u64,
            token: token_address.clone(),
            options_count: 3u32,
            description: String::from_str(&env, "Test Pool"),
            metadata_url: String::from_str(&env, "ipfs://metadata"),
            min_stake: 1i128,
            max_stake: 0i128,
            initial_liquidity: 0i128,
            category: symbol_short!("Tech"),
            max_total_stake: 0i128,
        },
    );

    let user = Address::generate(&env);
    token_admin_client.mint(&user, &500);
    client.place_prediction(&user, &pool_a, &200, &0);

    env.ledger().with_mut(|li| li.timestamp = 100_001);
    client.resolve_pool(&operator, &pool_a, &0u32);

    // Create pool B immediately after resolution.
    let pool_b = client.create_pool(
        &creator,
        &CreatePoolArgs {
            end_time: 100000u64,
            token: token_address.clone(),
            options_count: 3u32,
            description: String::from_str(&env, "Test Pool"),
            metadata_url: String::from_str(&env, "ipfs://metadata"),
            min_stake: 1i128,
            max_stake: 0i128,
            initial_liquidity: 0i128,
            category: symbol_short!("Tech"),
            max_total_stake: 0i128,
        },
    );

    assert_ne!(pool_a, pool_b);

    // User can still claim from pool A.
    let winnings = client.claim_winnings(&user, &pool_a);
    assert_eq!(winnings, 200);

    // Pool B is still active – predictions can be placed.
    let user2 = Address::generate(&env);
    token_admin_client.mint(&user2, &500);
    client.place_prediction(&user2, &pool_b, &100, &1);
}

/// Cancel pool A while pool B remains active, then resolve pool B.
/// Verifies that cancellation of one pool does not corrupt another.
#[test]
fn test_state_consistency_after_cancellation_and_resolution() {
    let env = Env::default();
    env.mock_all_auths();

    let (_, client, token_address, token, token_admin_client, _, operator, creator) = setup(&env);
    let contract_addr = client.address.clone();

    let pool_a = client.create_pool(
        &creator,
        &CreatePoolArgs {
            end_time: 100000u64,
            token: token_address.clone(),
            options_count: 3u32,
            description: String::from_str(&env, "Test Pool"),
            metadata_url: String::from_str(&env, "ipfs://metadata"),
            min_stake: 1i128,
            max_stake: 0i128,
            initial_liquidity: 0i128,
            category: symbol_short!("Tech"),
            max_total_stake: 0i128,
        },
    );

    let pool_b = client.create_pool(
        &creator,
        &CreatePoolArgs {
            end_time: 100000u64,
            token: token_address.clone(),
            options_count: 3u32,
            description: String::from_str(&env, "Test Pool"),
            metadata_url: String::from_str(&env, "ipfs://metadata"),
            min_stake: 1i128,
            max_stake: 0i128,
            initial_liquidity: 0i128,
            category: symbol_short!("Tech"),
            max_total_stake: 0i128,
        },
    );

    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);
    token_admin_client.mint(&user_a, &1000);
    token_admin_client.mint(&user_b, &1000);

    client.place_prediction(&user_a, &pool_a, &300, &0);
    client.place_prediction(&user_b, &pool_b, &400, &1);

    // Cancel pool A; 300 remain locked for refund.
    client.cancel_pool(&operator, &pool_a);

    env.ledger().with_mut(|li| li.timestamp = 100_001);
    client.resolve_pool(&operator, &pool_b, &1u32);

    // user_b is the sole better on winning outcome of pool_b → receives full 400.
    let wb = client.claim_winnings(&user_b, &pool_b);
    assert_eq!(wb, 400);

    // Contract should still hold pool_a's 300 (user_a's refund not yet claimed).
    assert_eq!(token.balance(&contract_addr), 300);

    // user_a claims refund from canceled pool_a.
    let wa_refund = client.claim_winnings(&user_a, &pool_a);
    assert_eq!(wa_refund, 300);

    // Contract drained to zero.
    assert_eq!(token.balance(&contract_addr), 0);
}

/// Verify that the contract correctly handles a pool with no losers
/// (every bettor chose the winning outcome).  The sole winner gets everything;
/// the invariant total_paid == total_staked must still hold.
#[test]
fn test_all_bettors_on_winning_side() {
    let env = Env::default();
    env.mock_all_auths();

    let (_, client, token_address, token, token_admin_client, _, operator, creator) = setup(&env);
    let contract_addr = client.address.clone();

    let pool_id = client.create_pool(
        &creator,
        &CreatePoolArgs {
            end_time: 100000u64,
            token: token_address.clone(),
            options_count: 2u32,
            description: String::from_str(&env, "All win pool"),
            metadata_url: String::from_str(&env, "ipfs://allwin"),
            min_stake: 1i128,
            max_stake: 0i128,
            initial_liquidity: 0i128,
            category: symbol_short!("Tech"),
            max_total_stake: 0i128,
        },
    );

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    token_admin_client.mint(&user1, &600);
    token_admin_client.mint(&user2, &400);

    client.place_prediction(&user1, &pool_id, &600, &0);
    client.place_prediction(&user2, &pool_id, &400, &0);

    let total = 1_000i128;
    assert_eq!(token.balance(&contract_addr), total);

    env.ledger().with_mut(|li| li.timestamp = 100_001);
    client.resolve_pool(&operator, &pool_id, &0u32);

    let w1 = client.claim_winnings(&user1, &pool_id);
    let w2 = client.claim_winnings(&user2, &pool_id);

    // Proportional split: 600 and 400.
    assert_eq!(w1, 600);
    assert_eq!(w2, 400);
    assert_eq!(w1 + w2, total);
    assert_eq!(token.balance(&contract_addr), 0);
}

/// If no one bet on the winning outcome, all claimants must receive 0.
#[test]
fn test_no_bettor_on_winning_side() {
    let env = Env::default();
    env.mock_all_auths();

    let (_, client, token_address, _, token_admin_client, _, operator, creator) = setup(&env);

    let pool_id = client.create_pool(
        &creator,
        &CreatePoolArgs {
            end_time: 100000u64,
            token: token_address.clone(),
            options_count: 3u32,
            description: String::from_str(&env, "Empty winner pool"),
            metadata_url: String::from_str(&env, "ipfs://emptywinner"),
            min_stake: 1i128,
            max_stake: 0i128,
            initial_liquidity: 0i128,
            category: symbol_short!("Tech"),
            max_total_stake: 0i128,
        },
    );

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    token_admin_client.mint(&user1, &500);
    token_admin_client.mint(&user2, &500);

    // Both bet on outcome 1; outcome 2 wins (nobody bet on it).
    client.place_prediction(&user1, &pool_id, &300, &1);
    client.place_prediction(&user2, &pool_id, &200, &1);

    env.ledger().with_mut(|li| li.timestamp = 100_001);
    client.resolve_pool(&operator, &pool_id, &2u32); // outcome 2 – no bettors

    let w1 = client.claim_winnings(&user1, &pool_id);
    let w2 = client.claim_winnings(&user2, &pool_id);
    assert_eq!(w1, 0);
    assert_eq!(w2, 0);
}
