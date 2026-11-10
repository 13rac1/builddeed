//! Runs the compiled contract on near-sandbox and checks the payments.
//!
//! Build the WASM first (`scripts/contract.sh build`). The test reads it from
//! `BUILDDEED_WASM`, or from the default release path.

use near_workspaces::types::NearToken;
use near_workspaces::{Account, Contract};
use serde_json::{json, Value};

fn wasm_path() -> String {
    std::env::var("BUILDDEED_WASM").unwrap_or_else(|_| {
        format!(
            "{}/target/wasm32-unknown-unknown/release/builddeed_license.wasm",
            env!("CARGO_MANIFEST_DIR")
        )
    })
}

async fn balance(a: &Account) -> anyhow::Result<u128> {
    Ok(a.view_account().await?.balance.as_yoctonear())
}

async fn new_account(root: &Account, name: &str) -> anyhow::Result<Account> {
    Ok(root
        .create_subaccount(name)
        .initial_balance(NearToken::from_near(30))
        .transact()
        .await?
        .into_result()?)
}

const NEAR: u128 = 10u128.pow(24);

#[tokio::test]
async fn sale_resale_and_entitlement_on_sandbox() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let wasm = std::fs::read(wasm_path())?;
    let root = worker.root_account()?;
    let dev = new_account(&root, "dev").await?;
    let contrib = new_account(&root, "contrib").await?;
    let fee = new_account(&root, "fee").await?;
    let alice = new_account(&root, "alice").await?;
    let bob = new_account(&root, "bob").await?;
    let contract: Contract = root
        .create_subaccount("demo")
        .initial_balance(NearToken::from_near(10))
        .transact()
        .await?
        .into_result()?
        .deploy(&wasm)
        .await?
        .into_result()?;

    contract
        .call("new")
        .args_json(json!({
            "config": {
                "product": "BuildDeed Checksum",
                "developer": dev.id(),
                "price": (5 * NEAR).to_string(),
                "renewal_price": (2 * NEAR).to_string(),
                "update_period_days": 365,
                "royalty_bps": 1000,
                "fee_account": fee.id(),
                "fee_bps": 300,
                "payees": [
                    { "account_id": dev.id(), "bps": 7000 },
                    { "account_id": contrib.id(), "bps": 3000 }
                ]
            },
            "symbol": "BDCK",
            "icon": null
        }))
        .transact()
        .await?
        .into_result()?;

    // The developer records a build.
    dev.call(contract.id(), "add_build")
        .args_json(json!({
            "version": "0.1.0",
            "commit": "a".repeat(40),
            "artifacts": [{
                "platform": "linux-x86_64",
                "file": "app.deb",
                "sha256": "b".repeat(64),
                "attestation_url": null
            }]
        }))
        .transact()
        .await?
        .into_result()?;

    // First sale: 3% fee, then 70/30 between developer and contributor,
    // less the storage of the new token.
    let fee0 = balance(&fee).await?;
    let contrib0 = balance(&contrib).await?;
    let out = alice
        .call(contract.id(), "buy")
        .deposit(NearToken::from_near(6))
        .max_gas()
        .transact()
        .await?;
    let license: Value = out.clone().into_result()?.json()?;
    assert_eq!(license["token_id"], "1");
    let fee_got = balance(&fee).await? - fee0;
    let contrib_got = balance(&contrib).await? - contrib0;
    // The fee is 3% of the price less storage: a little under 0.15 NEAR.
    assert!(fee_got < 15 * NEAR / 100 && fee_got > 14 * NEAR / 100, "fee {fee_got}");
    // The contributor gets 30% of the rest: a little under 1.455 NEAR.
    assert!(contrib_got < 1455 * NEAR / 1000 && contrib_got > 1400 * NEAR / 1000, "contrib {contrib_got}");

    let entitled: bool = contract
        .view("is_entitled")
        .args_json(json!({ "account_id": alice.id(), "version": "0.1.0" }))
        .await?
        .json()?;
    assert!(entitled);

    // A wallet transfer is refused.
    let refused = alice
        .call(contract.id(), "nft_transfer")
        .args_json(json!({ "receiver_id": bob.id(), "token_id": "1" }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?;
    assert!(refused.is_failure());

    // Resale for 3 NEAR: 3% fee, 10% royalty split 70/30, 87% to the seller.
    alice
        .call(contract.id(), "list")
        .args_json(json!({ "token_id": "1", "price": (3 * NEAR).to_string() }))
        .transact()
        .await?
        .into_result()?;
    let fee1 = balance(&fee).await?;
    let contrib1 = balance(&contrib).await?;
    let alice1 = balance(&alice).await?;
    bob.call(contract.id(), "buy_listed")
        .args_json(json!({ "token_id": "1" }))
        .deposit(NearToken::from_near(3))
        .max_gas()
        .transact()
        .await?
        .into_result()?;
    assert_eq!(balance(&fee).await? - fee1, 9 * NEAR / 100);
    assert_eq!(balance(&contrib).await? - contrib1, 9 * NEAR / 100);
    // The seller also gets the gas refund of the `list` call, which can
    // arrive after the balance read above: allow up to 0.02 NEAR more.
    let seller_got = balance(&alice).await? - alice1;
    assert!(seller_got >= 261 * NEAR / 100 && seller_got < 263 * NEAR / 100, "seller {seller_got}");

    let alice_now: bool = contract
        .view("is_entitled")
        .args_json(json!({ "account_id": alice.id(), "version": "0.1.0" }))
        .await?
        .json()?;
    let bob_now: bool = contract
        .view("is_entitled")
        .args_json(json!({ "account_id": bob.id(), "version": "0.1.0" }))
        .await?
        .json()?;
    assert!(!alice_now);
    assert!(bob_now);
    Ok(())
}
