#![cfg(test)]
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{testutils::{Address as _, Ledger}, Address, Env, Symbol};

mod mock_oracle_initial {
    use soroban_sdk::{contract, contractimpl, Address, Env};
    use crate::PriceData;
    #[contract] pub struct MockOracleInitial;
    #[contractimpl] impl MockOracleInitial {
        pub fn lastprice(_env: Env, _base: Address, _quote: Address) -> Option<PriceData> {
            Some(PriceData { price: 10_000_000, timestamp: 0 })
        }
    }
}

mod mock_oracle_stale {
    use soroban_sdk::{contract, contractimpl, Address, Env};
    use crate::PriceData;
    #[contract] pub struct MockOracleStale;
    #[contractimpl] impl MockOracleStale {
        pub fn lastprice(_env: Env, _base: Address, _quote: Address) -> Option<PriceData> {
            Some(PriceData { price: 5_000_000, timestamp: 0 })
        }
    }
}

fn setup_dynamic<'a>() -> (Env, AhjoorPaymentsContractClient<'a>, Address, Address, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorPaymentsContract, ());
    let client = AhjoorPaymentsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    
    // usdc_token
    let usdc_addr = env.register_stellar_asset_contract_v2(admin.clone()).address();
    // payment token
    let token_addr = env.register_stellar_asset_contract_v2(admin.clone()).address();

    let oracle_initial = env.register(mock_oracle_initial::MockOracleInitial, ());
    let oracle_stale = env.register(mock_oracle_stale::MockOracleStale, ());

    client.initialize(&admin, &admin, &0u32);
    client.set_min_collateral(&0i128);
    client.approve_merchant(&merchant);
    client.set_oracle(&oracle_initial, &usdc_addr, &3600u64);

    (env, client, admin, merchant, usdc_addr, token_addr, oracle_initial, oracle_stale)
}

#[test]
fn test_settlement_rejects_stale_oracle_price() {
    let (env, client, admin, merchant, usdc_addr, token_addr, oracle_initial, oracle_stale) = setup_dynamic();
    
    let customer = Address::generate(&env);
    let token_admin = soroban_sdk::token::StellarAssetClient::new(&env, &token_addr);
    token_admin.mint(&customer, &10_000_000);

    // Set slippage tolerance to 500 bps (5%)
    client.set_merchant_slippage_tolerance(&merchant, &500u32);

    // Create a multi-token payment
    // We are requesting 1_000_000 USDC. Oracle says 1 token = 1 USDC.
    // So 1_000_000 token_addr will be escrowed.
    let pid = client.create_payment_multi_token(
        &customer,
        &merchant,
        &1_000_000,
        &token_addr,
        &Some(500u32),
    );

    // Now, simulate that the oracle price drops to 0.5 USDC per token (50% drop, > 5% slippage).
    client.set_oracle(&oracle_stale, &usdc_addr, &3600u64);

    // Provide some USDC to the contract to fulfill the completion if it wasn't aborted
    let usdc_admin = soroban_sdk::token::StellarAssetClient::new(&env, &usdc_addr);
    usdc_admin.mint(&client.address, &1_000_000);

    // Attempt to settle payment
    // complete_payment should fail with SlippageExceeded (error code 21)
    let res = client.try_complete_payment(&pid);
    assert!(res.is_err());
    let err = res.unwrap_err().unwrap();
    // In soroban-sdk, contract errors are returned as soroban_sdk::Error. 
    // We can check if it's a contract error.
    assert!(err.is_type(soroban_sdk::xdr::ScErrorType::Contract));
    // error code 21
    assert_eq!(err.get_code(), 21);
}
