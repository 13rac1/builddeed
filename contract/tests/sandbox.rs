//! Runs the compiled contract on near-sandbox and checks the payments.
//!
//! Build the WASM first (`scripts/contract.sh build`). The test reads it from
//! `BUILDDEED_WASM`, or from the default release path.

use near_workspaces::result::ExecutionFinalResult;
use near_workspaces::types::{AccessKey, Gas, KeyType, NearToken, SecretKey};
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
/// Storage cost of one byte on NEAR: 10^19 yoctoNEAR.
const BYTE_COST: u128 = 10u128.pow(19);

/// The balance that is not locked for storage, and the storage in bytes.
async fn free_balance(a: &Account) -> anyhow::Result<(u128, u64)> {
    let v = a.view_account().await?;
    let usage = v.storage_usage;
    Ok((
        v.balance.as_yoctonear() - u128::from(usage) * BYTE_COST,
        usage,
    ))
}

async fn view(contract: &Contract, method: &str, args: Value) -> anyhow::Result<Value> {
    Ok(contract.view(method).args_json(args).await?.json()?)
}

/// Records one build with one artifact.
async fn add_build(
    contract: &Contract,
    dev: &Account,
    product: &str,
    commit: char,
    sha256: char,
) -> anyhow::Result<()> {
    dev.call(contract.id(), "add_build")
        .args_json(json!({
            "product": product,
            "version": "0.1.0",
            "commit": commit.to_string().repeat(40),
            "artifacts": [{
                "platform": "linux-x86_64",
                "file": "app.deb",
                "sha256": sha256.to_string().repeat(64),
                "attestation_url": null
            }]
        }))
        .transact()
        .await?
        .into_result()?;
    Ok(())
}

async fn is_entitled(
    contract: &Contract,
    account: &Account,
    product: &str,
) -> anyhow::Result<bool> {
    let v = view(
        contract,
        "is_entitled",
        json!({ "account_id": account.id(), "product": product, "version": "0.1.0" }),
    )
    .await?;
    Ok(v.as_bool().expect("is_entitled returns a bool"))
}

/// The budget balance of an account, in yoctoNEAR.
async fn budget_balance(contract: &Contract, account: &Account) -> anyhow::Result<u128> {
    let b = view(
        contract,
        "get_budget",
        json!({ "account_id": account.id() }),
    )
    .await?;
    Ok(b["balance"].as_str().expect("a budget").parse()?)
}

/// The error text of a call that must fail. A transaction that the network
/// refuses before it runs is a failure too.
fn failure(out: near_workspaces::Result<ExecutionFinalResult>, what: &str) -> String {
    match out {
        Err(e) => format!("{e:?}"),
        Ok(o) => format!("{:?}", o.into_result().expect_err(what)),
    }
}

#[tokio::test]
async fn two_products_sale_resale_and_isolation_on_sandbox() -> anyhow::Result<()> {
    let worker = near_workspaces::sandbox().await?;
    let wasm = std::fs::read(wasm_path())?;
    let root = worker.root_account()?;
    let dev = new_account(&root, "dev").await?;
    let dev2 = new_account(&root, "dev2").await?;
    let contrib = new_account(&root, "contrib").await?;
    let fee = new_account(&root, "fee").await?;
    let alice = new_account(&root, "alice").await?;
    let bob = new_account(&root, "bob").await?;
    let carol = new_account(&root, "carol").await?;
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
        .args_json(json!({ "fee_account": fee.id(), "fee_bps": 300, "icon": null }))
        .transact()
        .await?
        .into_result()?;

    // Two developers add one product each.
    let checksum: Value = dev
        .call(contract.id(), "add_product")
        .args_json(json!({
            "key": "checksum",
            "terms": {
                "name": "BuildDeed Checksum",
                "description": "Checks the SHA-256 of a file.",
                "url": "https://github.com/13rac1/builddeed",
                "price": (5 * NEAR).to_string(),
                "renewal_price": (2 * NEAR).to_string(),
                "update_period_days": 365,
                "royalty_bps": 1000,
                "payees": [
                    { "account_id": dev.id(), "bps": 7000 },
                    { "account_id": contrib.id(), "bps": 3000 }
                ],
                "max_devices": 3
            }
        }))
        .deposit(NearToken::from_near(1))
        .transact()
        .await?
        .into_result()?
        .json()?;
    assert_eq!(checksum["developer"], dev.id().as_str());
    let checksum_balance: u128 = checksum["storage_balance"].as_str().unwrap().parse()?;
    assert!(
        checksum_balance > 0 && checksum_balance < NEAR,
        "{checksum_balance}"
    );
    dev2.call(contract.id(), "add_product")
        .args_json(json!({
            "key": "doom",
            "terms": {
                "name": "Iron Doom",
                "description": "",
                "url": null,
                "price": (3 * NEAR).to_string(),
                "renewal_price": NEAR.to_string(),
                "update_period_days": 30,
                "royalty_bps": 500,
                "payees": [{ "account_id": dev2.id(), "bps": 10000 }],
                "max_devices": 1
            }
        }))
        .deposit(NearToken::from_near(1))
        .transact()
        .await?
        .into_result()?;
    let products = view(&contract, "get_products", json!({})).await?;
    assert_eq!(products[0]["key"], "checksum");
    assert_eq!(products[1]["key"], "doom");
    assert_eq!(products[1]["developer"], dev2.id().as_str());

    // dev2 cannot record a build of dev's product.
    let foreign = dev2
        .call(contract.id(), "add_build")
        .args_json(json!({
            "product": "checksum",
            "version": "0.1.0",
            "commit": "c".repeat(40),
            "artifacts": [{ "platform": "linux-x86_64", "file": "x.deb",
                            "sha256": "e".repeat(64), "attestation_url": null }]
        }))
        .transact()
        .await?;
    let err = format!(
        "{:?}",
        foreign
            .into_result()
            .expect_err("a foreign add_build must fail")
    );
    assert!(err.contains("Only the developer of this product"), "{err}");

    // Each developer records a build. The two builds contain the same file.
    add_build(&contract, &dev, "checksum", 'a', 'b').await?;
    add_build(&contract, &dev2, "doom", 'c', 'b').await?;
    let after_build = view(&contract, "get_product", json!({ "key": "checksum" })).await?;
    let after_build: u128 = after_build["storage_balance"].as_str().unwrap().parse()?;
    assert!(
        after_build < checksum_balance,
        "the build storage comes from the storage balance"
    );
    for (product, commit) in [("checksum", 'a'), ("doom", 'c')] {
        let m = view(
            &contract,
            "find_artifact",
            json!({ "product": product, "sha256": "b".repeat(64) }),
        )
        .await?;
        assert_eq!(m["commit"], commit.to_string().repeat(40), "{product}");
    }

    // First sale: 3% fee, then 70/30 between developer and contributor.
    // The storage of the new license comes out of the price first.
    // Let the gas refunds of dev2's calls arrive before the balance read.
    worker.fast_forward(5).await?;
    let fee0 = balance(&fee).await?;
    let contrib0 = balance(&contrib).await?;
    let dev2_0 = balance(&dev2).await?;
    let (free0, usage0) = free_balance(contract.as_account()).await?;
    let license: Value = alice
        .call(contract.id(), "buy")
        .args_json(json!({ "product": "checksum" }))
        .deposit(NearToken::from_near(6))
        .max_gas()
        .transact()
        .await?
        .into_result()?
        .json()?;
    assert_eq!(license["token_id"], "checksum:1");
    let (free1, usage1) = free_balance(contract.as_account()).await?;
    let storage_cost = u128::from(usage1 - usage0) * BYTE_COST;
    let rest = 5 * NEAR - storage_cost;
    let fee_part = rest * 300 / 10_000;
    assert_eq!(
        balance(&fee).await? - fee0,
        fee_part,
        "fee of the first sale"
    );
    assert_eq!(
        balance(&contrib).await? - contrib0,
        (rest - fee_part) * 3000 / 10_000,
        "contributor share of the first sale"
    );
    assert_eq!(
        balance(&dev2).await?,
        dev2_0,
        "the other developer gets nothing"
    );
    // The buyer pays the storage: the contract's free balance does not go down.
    assert!(free1 >= free0, "free balance went down: {free0} -> {free1}");
    assert!(is_entitled(&contract, &alice, "checksum").await?);
    assert!(
        !is_entitled(&contract, &alice, "doom").await?,
        "a license of checksum does not cover doom"
    );

    // Alice also buys a license of doom: its numbers start at 1.
    let doom: Value = alice
        .call(contract.id(), "buy")
        .args_json(json!({ "product": "doom" }))
        .deposit(NearToken::from_near(3))
        .max_gas()
        .transact()
        .await?
        .into_result()?
        .json()?;
    assert_eq!(doom["token_id"], "doom:1");
    let all = view(
        &contract,
        "licenses_for_owner",
        json!({ "account_id": alice.id() }),
    )
    .await?;
    assert_eq!(all.as_array().unwrap().len(), 2);
    let only_doom = view(
        &contract,
        "licenses_for_owner",
        json!({ "account_id": alice.id(), "product": "doom" }),
    )
    .await?;
    assert_eq!(only_doom.as_array().unwrap().len(), 1);
    assert_eq!(only_doom[0]["token_id"], "doom:1");

    // Alice registers one computer on both licenses. She pays its storage:
    // the contract's free balance does not go down.
    let device = "d".repeat(64);
    let (free_a, _) = free_balance(contract.as_account()).await?;
    for token in ["checksum:1", "doom:1"] {
        alice
            .call(contract.id(), "add_device")
            .args_json(json!({ "token_id": token, "device_id": device }))
            .deposit(NearToken::from_millinear(10))
            .transact()
            .await?
            .into_result()?;
    }
    let (free_b, _) = free_balance(contract.as_account()).await?;
    assert!(
        free_b >= free_a,
        "free balance went down: {free_a} -> {free_b}"
    );
    for (product, token) in [("checksum", "checksum:1"), ("doom", "doom:1")] {
        let seat = view(
            &contract,
            "device_license",
            json!({ "product": product, "device_id": device, "version": "0.1.0" }),
        )
        .await?;
        assert_eq!(seat["token_id"], token);
        assert_eq!(seat["owner_id"], alice.id().as_str());
    }

    // A wallet transfer is refused with the market message.
    let refused = alice
        .call(contract.id(), "nft_transfer")
        .args_json(json!({ "receiver_id": bob.id(), "token_id": "checksum:1" }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?;
    let err = format!(
        "{:?}",
        refused.into_result().expect_err("nft_transfer must fail")
    );
    assert!(
        err.contains("change owner only through the market"),
        "{err}"
    );

    // A listing below the renewal price is refused.
    let low = alice
        .call(contract.id(), "list")
        .args_json(json!({ "token_id": "checksum:1", "price": "1" }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?;
    let err = format!(
        "{:?}",
        low.into_result().expect_err("a 1 yocto listing must fail")
    );
    assert!(err.contains("at least the renewal price"), "{err}");

    // Resale for 3 NEAR: 3% fee, 10% royalty split 70/30, 87% to the seller.
    alice
        .call(contract.id(), "list")
        .args_json(json!({ "token_id": "checksum:1", "price": (3 * NEAR).to_string() }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;
    let listed = view(&contract, "get_listings", json!({ "product": "checksum" })).await?;
    assert_eq!(listed.as_array().unwrap().len(), 1);
    let none = view(&contract, "get_listings", json!({ "product": "doom" })).await?;
    assert!(none.as_array().unwrap().is_empty());
    // Let the gas refund of the list call arrive before the balance read.
    worker.fast_forward(5).await?;
    let fee1 = balance(&fee).await?;
    let contrib1 = balance(&contrib).await?;
    let alice1 = balance(&alice).await?;
    let dev2_1 = balance(&dev2).await?;
    bob.call(contract.id(), "buy_listed")
        .args_json(json!({ "token_id": "checksum:1", "price": (3 * NEAR).to_string() }))
        .deposit(NearToken::from_near(3))
        .max_gas()
        .transact()
        .await?
        .into_result()?;
    assert_eq!(
        balance(&fee).await? - fee1,
        9 * NEAR / 100,
        "fee of the resale"
    );
    assert_eq!(
        balance(&contrib).await? - contrib1,
        9 * NEAR / 100,
        "contributor royalty"
    );
    assert_eq!(balance(&alice).await? - alice1, 261 * NEAR / 100, "seller");
    assert_eq!(
        balance(&dev2).await?,
        dev2_1,
        "no royalty to the other product"
    );

    assert!(
        !is_entitled(&contract, &alice, "checksum").await?,
        "the seller is not entitled after the resale"
    );
    assert!(
        is_entitled(&contract, &bob, "checksum").await?,
        "the buyer is entitled after the resale"
    );
    assert!(
        is_entitled(&contract, &alice, "doom").await?,
        "the seller keeps her license of doom"
    );
    let seat = view(
        &contract,
        "device_license",
        json!({ "product": "checksum", "device_id": device, "version": "0.1.0" }),
    )
    .await?;
    assert!(seat.is_null(), "the resale cleared the seller's computer");
    let seat = view(
        &contract,
        "device_license",
        json!({ "product": "doom", "device_id": device, "version": "0.1.0" }),
    )
    .await?;
    assert_eq!(seat["token_id"], "doom:1", "the seat in doom stays");
    assert_eq!(seat["covers_version"], true);
    // Agent budget. Carol deposits with her full-access key, then gives an
    // agent a function-call key that can call only the two budget methods.
    carol
        .call(contract.id(), "deposit_budget")
        .args_json(json!({ "max_price": (5 * NEAR).to_string(), "products": ["checksum"] }))
        .deposit(NearToken::from_near(12))
        .transact()
        .await?
        .into_result()?;
    let budget0 = budget_balance(&contract, &carol).await?;
    assert!(budget0 > 11 * NEAR && budget0 < 12 * NEAR, "{budget0}");
    let agent_key = SecretKey::from_random(KeyType::ED25519);
    carol
        .batch(carol.id())
        .add_key(
            agent_key.public_key(),
            AccessKey::function_call_access(
                contract.id(),
                &["buy_from_budget", "add_device_from_budget"],
                Some(NearToken::from_near(1)),
            ),
        )
        .transact()
        .await?
        .into_result()?;
    let agent = Account::from_secret_key(carol.id().clone(), agent_key, &worker);

    let bought: Value = agent
        .call(contract.id(), "buy_from_budget")
        .args_json(json!({ "product": "checksum" }))
        // The prepaid gas must fit in the allowance of the key.
        .gas(Gas::from_tgas(300))
        .transact()
        .await?
        .into_result()?
        .json()?;
    assert_eq!(bought["token_id"], "checksum:2");
    assert_eq!(bought["owner_id"], carol.id().as_str());
    let budget1 = budget_balance(&contract, &carol).await?;
    assert_eq!(budget1, budget0 - 5 * NEAR, "the budget pays the price");

    let agent_device = "f".repeat(64);
    agent
        .call(contract.id(), "add_device_from_budget")
        .args_json(json!({ "token_id": "checksum:2", "device_id": agent_device }))
        .transact()
        .await?
        .into_result()?;
    let budget2 = budget_balance(&contract, &carol).await?;
    assert!(
        budget2 < budget1 && budget1 - budget2 < NEAR / 100,
        "the budget pays the seat storage: {budget1} -> {budget2}"
    );
    let seat = view(
        &contract,
        "device_license",
        json!({ "product": "checksum", "device_id": agent_device, "version": "0.1.0" }),
    )
    .await?;
    assert_eq!(seat["token_id"], "checksum:2");

    // The function-call key cannot withdraw the budget or list the license.
    let withdraw = agent
        .call(contract.id(), "withdraw_budget")
        .args_json(json!({}))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await;
    let err = failure(
        withdraw,
        "withdraw_budget with a function-call key must fail",
    );
    assert!(err.contains("DepositWithFunctionCall"), "{err}");
    // Without the deposit, the key refuses the method name.
    let withdraw = agent
        .call(contract.id(), "withdraw_budget")
        .args_json(json!({}))
        .transact()
        .await;
    let err = failure(
        withdraw,
        "withdraw_budget with a function-call key must fail",
    );
    assert!(err.contains("MethodNameMismatch"), "{err}");
    let list = agent
        .call(contract.id(), "list")
        .args_json(json!({ "token_id": "checksum:2", "price": (3 * NEAR).to_string() }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await;
    let err = failure(list, "list with a function-call key must fail");
    assert!(err.contains("DepositWithFunctionCall"), "{err}");
    assert_eq!(budget_balance(&contract, &carol).await?, budget2);
    let license = view(
        &contract,
        "get_license",
        json!({ "token_id": "checksum:2" }),
    )
    .await?;
    assert_eq!(license["owner_id"], carol.id().as_str());
    assert!(
        license["listed_price"].is_null(),
        "the license is not listed"
    );

    // The full-access key can withdraw all of it.
    carol
        .call(contract.id(), "withdraw_budget")
        .args_json(json!({}))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?
        .into_result()?;
    assert_eq!(budget_balance(&contract, &carol).await?, 0);
    Ok(())
}
