use super::*;
use near_sdk::mock::MockAction;
use near_sdk::test_utils::{get_created_receipts, get_logs, VMContextBuilder};
use near_sdk::testing_env;

const DAY_MS: u64 = 24 * 60 * 60 * 1000;
const YOCTO: u128 = 1;
/// The first product: developer dev.near.
const P: &str = "checksum";
/// The second product: developer dev2.near.
const Q: &str = "doom";
/// The first license of product P.
const T1: &str = "checksum:1";

fn acc(s: &str) -> AccountId {
    s.parse().unwrap()
}

fn near(n: u128) -> NearToken {
    NearToken::from_near(n)
}

fn yocto(n: u128) -> NearToken {
    NearToken::from_yoctonear(n)
}

/// The terms of product P.
fn terms() -> ProductTerms {
    ProductTerms {
        name: "BuildDeed Checksum".into(),
        description: "Checks the SHA-256 of a file.".into(),
        url: Some("https://github.com/13rac1/builddeed".into()),
        price: near(5),
        renewal_price: near(2),
        update_period_days: 365,
        royalty_bps: 1000,
        payees: vec![
            Payee {
                account_id: acc("dev.near"),
                bps: 7000,
            },
            Payee {
                account_id: acc("contrib.near"),
                bps: 3000,
            },
        ],
        max_devices: 3,
    }
}

/// The terms of product Q: other prices, period, royalty, payees and seats.
fn terms2() -> ProductTerms {
    ProductTerms {
        name: "Iron Doom".into(),
        description: String::new(),
        url: None,
        price: near(3),
        renewal_price: near(1),
        update_period_days: 30,
        royalty_bps: 500,
        payees: vec![Payee {
            account_id: acc("dev2.near"),
            bps: 10_000,
        }],
        max_devices: 1,
    }
}

/// Sets the caller, deposit and block time (ms) for the next call.
fn call(who: &str, deposit: NearToken, now_ms: u64) {
    let mut b = VMContextBuilder::new();
    b.current_account_id(acc("demo.near"))
        .predecessor_account_id(acc(who))
        .signer_account_id(acc(who))
        .attached_deposit(deposit)
        .account_balance(near(100))
        .block_timestamp(now_ms * 1_000_000);
    testing_env!(b.build());
}

/// A contract with a 3% fee and product P of dev.near.
fn setup() -> Contract {
    call("owner.near", yocto(0), 1_000);
    let mut c = Contract::new(acc("fee.near"), 300, None);
    call("dev.near", near(1), 1_000);
    c.add_product(P.into(), terms());
    c
}

/// `setup` and product Q of dev2.near.
fn setup_two() -> Contract {
    let mut c = setup();
    call("dev2.near", near(1), 1_000);
    c.add_product(Q.into(), terms2());
    c
}

/// A contract with license T1 owned by alice, bought at 10 000 ms.
fn with_license() -> Contract {
    let mut c = setup();
    call("alice.near", near(5), 10_000);
    c.buy(P.into());
    c
}

fn sha(c: char) -> String {
    c.to_string().repeat(64)
}

fn commit(c: char) -> String {
    c.to_string().repeat(40)
}

fn artifact(c: char) -> Artifact {
    Artifact {
        platform: "linux-x86_64".into(),
        file: format!("app-{c}.deb"),
        sha256: sha(c),
        attestation_url: Some("https://github.com/13rac1/builddeed/attestations/1".into()),
    }
}

fn total(parts: &[(AccountId, u128)]) -> u128 {
    parts.iter().map(|(_, a)| a).sum()
}

fn amount_of(parts: &[(AccountId, u128)], who: &str) -> u128 {
    parts
        .iter()
        .filter(|(a, _)| a.as_str() == who)
        .map(|(_, v)| v)
        .sum()
}

/// The NEAR transfers that the last call made, as (receiver, yoctoNEAR).
fn transfers() -> Vec<(AccountId, u128)> {
    get_created_receipts()
        .into_iter()
        .flat_map(|r| {
            let to = r.receiver_id.clone();
            r.actions.into_iter().filter_map(move |a| match a {
                MockAction::Transfer { deposit, .. } => Some((to.clone(), deposit.as_yoctonear())),
                _ => None,
            })
        })
        .collect()
}

/// The storage cost of the bytes written since `before`.
fn cost_since(before: u64) -> u128 {
    u128::from(env::storage_usage() - before) * env::storage_byte_cost().as_yoctonear()
}

fn panic_text(f: impl FnOnce()) -> String {
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    match r {
        Ok(()) => String::new(),
        Err(e) => e
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_default(),
    }
}

fn storage_balance(c: &Contract, product: &str) -> u128 {
    c.get_product(product.into())
        .unwrap()
        .storage_balance
        .as_yoctonear()
}

// ----- Settings -----

#[test]
fn new_sets_the_fee_and_the_collection_metadata() {
    let c = setup();
    let s = c.get_settings();
    assert_eq!((s.fee_account, s.fee_bps), (acc("fee.near"), 300));
    let m = c.nft_metadata();
    assert_eq!(
        (m.name.as_str(), m.symbol.as_str()),
        ("BuildDeed licenses", "DEED")
    );
}

#[test]
fn new_accepts_a_fee_of_10_percent() {
    call("owner.near", yocto(0), 1_000);
    let c = Contract::new(acc("fee.near"), MAX_FEE_BPS, None);
    assert_eq!(c.get_settings().fee_bps, 1_000);
}

#[test]
#[should_panic(expected = "The fee must be at most 10%")]
fn new_refuses_a_fee_above_10_percent() {
    call("owner.near", yocto(0), 1_000);
    Contract::new(acc("fee.near"), MAX_FEE_BPS + 1, None);
}

// ----- Products -----

#[test]
fn add_product_makes_the_caller_the_developer_and_keeps_the_rest_as_storage_balance() {
    call("owner.near", yocto(0), 1_000);
    let mut c = Contract::new(acc("fee.near"), 300, None);
    call("dev.near", near(1), 1_000);
    let before = env::storage_usage();
    let v = c.add_product(P.into(), terms());
    let record = cost_since(before);
    assert!(record > 0);
    assert_eq!(v.key, P);
    assert_eq!(v.developer, acc("dev.near"));
    assert_eq!(
        v.storage_balance.as_yoctonear(),
        near(1).as_yoctonear() - record
    );
    assert_eq!(v.name, "BuildDeed Checksum");
    assert_eq!((v.price, v.renewal_price), (near(5), near(2)));
    assert_eq!(
        (v.update_period_days, v.royalty_bps, v.max_devices),
        (365, 1000, 3)
    );
    assert_eq!(v.payees, terms().payees);
    assert_eq!((v.fee_account, v.fee_bps), (acc("fee.near"), 300));
    assert_eq!(v.licenses_sold, 0);
    let logs = get_logs();
    assert!(
        logs.iter()
            .any(|l| l.contains("\"event\":\"add_product\"") && l.contains("dev.near")),
        "{logs:?}"
    );
}

#[test]
fn product_key_rules() {
    let max = "a".repeat(32);
    let long = "a".repeat(33);
    for good in ["ab", "1-a", "a-", "doom-2", max.as_str()] {
        assert!(is_product_key(good), "{good}");
    }
    for bad in [
        "",
        "a",
        long.as_str(),
        "Doom",
        "-ab",
        "ab:1",
        "a_b",
        "a b",
        "a.b",
    ] {
        assert!(!is_product_key(bad), "{bad}");
    }
}

#[test]
fn add_product_refuses_a_bad_key() {
    let mut c = setup();
    let long = "a".repeat(33);
    for bad in ["a", long.as_str(), "Doom", "-ab", "ab:1"] {
        call("dev2.near", near(1), 1_000);
        let msg = panic_text(|| {
            c.add_product(bad.into(), terms2());
        });
        assert!(msg.contains("The product key must have"), "{bad}: {msg}");
    }
}

#[test]
#[should_panic(expected = "A product with this key exists")]
fn add_product_refuses_a_key_in_use() {
    let mut c = setup();
    call("dev2.near", near(1), 1_000);
    c.add_product(P.into(), terms2());
}

#[test]
#[should_panic(expected = "The payee shares must add up to 10000")]
fn add_product_checks_the_terms() {
    let mut c = setup();
    let mut t = terms2();
    t.payees[0].bps = 9_999;
    call("dev2.near", near(1), 1_000);
    c.add_product(Q.into(), t);
}

#[test]
#[should_panic(expected = "Attach at least")]
fn add_product_refuses_a_deposit_below_the_record_cost() {
    let mut c = setup();
    call("dev2.near", yocto(1), 1_000);
    c.add_product(Q.into(), terms2());
}

#[test]
fn get_products_lists_in_the_order_of_addition_and_pages() {
    let c = setup_two();
    let keys: Vec<String> = c
        .get_products(None, None)
        .into_iter()
        .map(|p| p.key)
        .collect();
    assert_eq!(keys, vec![P, Q]);
    let page: Vec<String> = c
        .get_products(Some(1), Some(1))
        .into_iter()
        .map(|p| p.key)
        .collect();
    assert_eq!(page, vec![Q]);
    assert!(c.get_products(Some(2), None).is_empty());
    assert!(c.get_product("nothing".into()).is_none());
    assert_eq!(c.get_product(Q.into()).unwrap().developer, acc("dev2.near"));
}

/// A change to good terms, and the panic text that it must give.
type TermsCase = (fn(&mut ProductTerms), &'static str);

/// Each terms rule, with the panic text that it must give.
#[test]
fn terms_checks() {
    let cases: Vec<TermsCase> = vec![
        (|t| t.name = String::new(), "The product name must have"),
        (|t| t.name = "p".repeat(65), "The product name must have"),
        (
            |t| t.description = "d".repeat(281),
            "at most 280 characters",
        ),
        (|t| t.url = Some("http://x".into()), "The URL must be https"),
        (
            |t| t.url = Some(format!("https://{}", "x".repeat(505))),
            "The URL must be https",
        ),
        (|t| t.price = yocto(0), "The price must be more than zero"),
        (
            |t| t.renewal_price = yocto(0),
            "The renewal price must be more than zero",
        ),
        (|t| t.update_period_days = 0, "update period"),
        (|t| t.update_period_days = 36_501, "update period"),
        (|t| t.royalty_bps = 9701, "at most 100%"),
        (|t| t.payees = vec![], "Give 1 to 16 payees"),
        (
            |t| {
                t.payees = (0..17)
                    .map(|i| Payee {
                        account_id: acc(&format!("p{i}.near")),
                        bps: 1,
                    })
                    .collect()
            },
            "Give 1 to 16 payees",
        ),
        (|t| t.payees[0].bps = 6999, "add up to 10000"),
        (|t| t.max_devices = 0, "Give 1 to 16 computers"),
        (
            |t| t.payees[1].account_id = t.payees[0].account_id.clone(),
            "Each payee account can appear only once",
        ),
        (|t| t.max_devices = 17, "Give 1 to 16 computers"),
    ];
    call("dev.near", yocto(0), 1_000);
    for (change, want) in cases {
        let mut t = terms();
        change(&mut t);
        let msg = panic_text(|| check_terms(&t, 300));
        assert!(msg.contains(want), "{want}: got {msg:?}");
    }
    // The limits themselves are allowed.
    let mut t = terms();
    t.description = "d".repeat(280);
    t.url = Some(format!("https://{}", "x".repeat(504)));
    t.update_period_days = 36_500;
    t.royalty_bps = 9700;
    t.max_devices = 16;
    assert_eq!(panic_text(|| check_terms(&t, 300)), "");
    assert_eq!(panic_text(|| check_terms(&terms(), 300)), "");
}

#[test]
fn set_prices_and_set_payees_change_the_terms() {
    let mut c = setup();
    call("dev.near", yocto(0), 5_000);
    c.set_prices(P.into(), near(7), near(3));
    c.set_payees(
        P.into(),
        vec![Payee {
            account_id: acc("dev.near"),
            bps: 10_000,
        }],
    );
    let v = c.get_product(P.into()).unwrap();
    assert_eq!((v.price, v.renewal_price), (near(7), near(3)));
    assert_eq!(v.payees.len(), 1);
}

#[test]
#[should_panic(expected = "The renewal price must be more than zero")]
fn set_prices_refuses_a_free_renewal() {
    let mut c = setup();
    call("dev.near", yocto(0), 5_000);
    c.set_prices(P.into(), near(5), yocto(0));
}

#[test]
#[should_panic(expected = "Only the developer")]
fn set_prices_by_other_account_fails() {
    let mut c = setup();
    call("alice.near", yocto(0), 5_000);
    c.set_prices(P.into(), near(1), near(1));
}

#[test]
#[should_panic(expected = "No product has this key")]
fn set_prices_of_unknown_product_fails() {
    let mut c = setup();
    call("dev.near", yocto(0), 5_000);
    c.set_prices("nothing".into(), near(1), near(1));
}

// ----- Storage balance -----

/// A contract with product P and product "zero" of dev2.near, whose
/// storage balance is 0: dev2 attached the exact cost of the record.
fn with_zero_balance() -> Contract {
    let mut c = setup();
    // "zero" and "aaaa" have the same key length and terms, so their
    // records have the same size.
    call("dev2.near", near(1), 1_000);
    let v = c.add_product("aaaa".into(), terms2());
    let record = near(1).as_yoctonear() - v.storage_balance.as_yoctonear();
    call("dev2.near", yocto(record), 1_000);
    let v = c.add_product("zero".into(), terms2());
    assert_eq!(v.storage_balance, yocto(0));
    c
}

#[test]
#[should_panic(expected = "The storage balance of this product is too low")]
fn add_build_fails_when_the_storage_balance_is_too_low() {
    let mut c = with_zero_balance();
    call("dev2.near", yocto(0), 5_000);
    c.add_build(
        "zero".into(),
        "0.1.0".into(),
        commit('a'),
        vec![artifact('a')],
    );
}

#[test]
#[should_panic(expected = "The storage balance of this product is too low")]
fn set_payees_fails_when_the_storage_balance_is_too_low() {
    let mut c = with_zero_balance();
    call("dev2.near", yocto(0), 5_000);
    c.set_payees(
        "zero".into(),
        vec![
            Payee {
                account_id: acc("dev2.near"),
                bps: 5_000,
            },
            Payee {
                account_id: acc("contrib.near"),
                bps: 5_000,
            },
        ],
    );
}

#[test]
fn add_storage_raises_the_balance_and_a_build_takes_its_storage_from_it() {
    let mut c = with_zero_balance();
    // Anyone can pay.
    call("alice.near", near(1), 5_000);
    assert_eq!(c.add_storage("zero".into()), near(1));
    call("alice.near", near(1), 5_000);
    assert_eq!(c.add_storage("zero".into()), near(2));
    assert_eq!(storage_balance(&c, "zero"), near(2).as_yoctonear());
    call("dev2.near", yocto(0), 6_000);
    let before = env::storage_usage();
    c.add_build(
        "zero".into(),
        "0.1.0".into(),
        commit('a'),
        vec![artifact('a')],
    );
    let used = cost_since(before);
    assert!(used > 0);
    assert_eq!(storage_balance(&c, "zero"), near(2).as_yoctonear() - used);
    // The other product's balance does not change.
    let p_before = storage_balance(&c, P);
    call("alice.near", near(1), 6_000);
    c.add_storage("zero".into());
    assert_eq!(storage_balance(&c, P), p_before);
}

#[test]
#[should_panic(expected = "No product has this key")]
fn add_storage_for_unknown_product_fails() {
    let mut c = setup();
    call("alice.near", near(1), 5_000);
    c.add_storage("nothing".into());
}

#[test]
fn set_payees_charges_growth_and_returns_freed_storage() {
    let mut c = setup();
    let start = storage_balance(&c, P);
    let three = vec![
        Payee {
            account_id: acc("dev.near"),
            bps: 5_000,
        },
        Payee {
            account_id: acc("contrib.near"),
            bps: 3_000,
        },
        Payee {
            account_id: acc("third.near"),
            bps: 2_000,
        },
    ];
    call("dev.near", yocto(0), 5_000);
    let before = env::storage_usage();
    c.set_payees(P.into(), three);
    let grown = cost_since(before);
    assert!(grown > 0);
    assert_eq!(storage_balance(&c, P), start - grown);
    call("dev.near", yocto(0), 5_000);
    c.set_payees(P.into(), terms().payees);
    assert_eq!(storage_balance(&c, P), start, "freed storage comes back");
}

// ----- Buy and renew -----

#[test]
fn buy_mints_a_license_with_one_update_period() {
    let mut c = setup();
    call("alice.near", near(5), 10_000);
    let l = c.buy(P.into());
    assert_eq!(l.token_id, T1);
    assert_eq!(l.product, P);
    assert_eq!(l.owner_id, acc("alice.near"));
    assert_eq!(l.purchased_at_ms, 10_000);
    assert_eq!(l.updates_until_ms, 10_000 + 365 * DAY_MS);
    assert_eq!(c.licenses_for_owner(acc("alice.near"), None).len(), 1);
    assert_eq!(c.nft_total_supply(), U128(1));
    assert_eq!(c.get_product(P.into()).unwrap().licenses_sold, 1);
}

#[test]
fn token_ids_count_per_product() {
    let mut c = setup_two();
    call("alice.near", near(5), 10_000);
    assert_eq!(c.buy(P.into()).token_id, "checksum:1");
    call("bob.near", near(3), 10_000);
    assert_eq!(c.buy(Q.into()).token_id, "doom:1");
    call("bob.near", near(5), 10_000);
    assert_eq!(c.buy(P.into()).token_id, "checksum:2");
    call("alice.near", near(3), 10_000);
    let l = c.buy(Q.into());
    assert_eq!(l.token_id, "doom:2");
    // Product Q has its own update period.
    assert_eq!(l.updates_until_ms, 10_000 + 30 * DAY_MS);
    assert_eq!(c.get_product(P.into()).unwrap().licenses_sold, 2);
    assert_eq!(c.get_product(Q.into()).unwrap().licenses_sold, 2);
    assert_eq!(c.nft_total_supply(), U128(4));
}

#[test]
fn token_metadata_extra_is_json_with_the_license_period() {
    let c = with_license();
    let token = c.nft_token(T1.into()).expect("token");
    let meta = token.metadata.expect("metadata");
    assert_eq!(meta.title.as_deref(), Some("BuildDeed Checksum license #1"));
    let extra = meta.extra.expect("extra");
    let v: near_sdk::serde_json::Value =
        near_sdk::serde_json::from_str(&extra).expect("valid JSON");
    assert_eq!(v["product"], P);
    assert_eq!(v["updates_until_ms"], 10_000 + 365 * DAY_MS);
}

#[test]
fn product_name_with_quote_and_backslash_gives_valid_metadata() {
    let mut c = setup();
    let mut t = terms2();
    t.name = "Odd \"name\\".into();
    call("dev2.near", near(1), 1_000);
    c.add_product("odd".into(), t);
    call("alice.near", near(3), 10_000);
    c.buy("odd".into());
    let meta = c.nft_token("odd:1".into()).unwrap().metadata.unwrap();
    assert_eq!(meta.title.as_deref(), Some("Odd \"name\\ license #1"));
    let v: near_sdk::serde_json::Value =
        near_sdk::serde_json::from_str(&meta.extra.unwrap()).expect("valid JSON");
    assert_eq!(v["product"], "odd");
}

#[test]
#[should_panic(expected = "Attach at least")]
fn buy_with_too_small_deposit_fails() {
    let mut c = setup();
    call(
        "alice.near",
        NearToken::from_yoctonear(near(5).as_yoctonear() - 1),
        10_000,
    );
    c.buy(P.into());
}

#[test]
#[should_panic(expected = "No product has this key")]
fn buy_of_unknown_product_fails() {
    let mut c = setup();
    call("alice.near", near(5), 10_000);
    c.buy("nothing".into());
}

#[test]
fn buy_pays_the_fee_and_the_payees_of_its_product_and_returns_the_excess() {
    let mut c = setup_two();
    for (product, buyer, price, deposit) in [
        (P, "alice.near", near(5), near(6)),
        (Q, "bob.near", near(3), near(3)),
    ] {
        call(buyer, deposit, 10_000);
        let before = env::storage_usage();
        c.buy(product.into());
        let rest = price.as_yoctonear() - cost_since(before);
        let paid = transfers();
        let fee = rest * 300 / 10_000;
        assert_eq!(amount_of(&paid, "fee.near"), fee, "{product} fee");
        let after_fee = rest - fee;
        if product == P {
            assert_eq!(amount_of(&paid, "contrib.near"), after_fee * 3 / 10);
            assert_eq!(amount_of(&paid, "dev.near"), after_fee - after_fee * 3 / 10);
            assert_eq!(amount_of(&paid, "dev2.near"), 0);
        } else {
            assert_eq!(amount_of(&paid, "dev2.near"), after_fee);
            assert_eq!(amount_of(&paid, "dev.near"), 0);
            assert_eq!(amount_of(&paid, "contrib.near"), 0);
        }
        assert_eq!(
            amount_of(&paid, buyer),
            deposit.as_yoctonear() - price.as_yoctonear(),
            "{product} excess"
        );
    }
}

#[test]
fn renew_starts_from_updates_until_or_now_and_updates_the_metadata() {
    let mut c = with_license();
    // Renew before the end: the period is added to updates_until.
    call("alice.near", near(2), 20_000);
    let l = c.renew(T1.into());
    assert_eq!(
        l.updates_until_ms,
        10_000 + 2 * 365 * DAY_MS,
        "renew before the end"
    );
    // Renew after the end: the period starts now.
    let late = 10_000 + 3 * 365 * DAY_MS;
    call("alice.near", near(2), late);
    let l = c.renew(T1.into());
    assert_eq!(
        l.updates_until_ms,
        late + 365 * DAY_MS,
        "renew after the end"
    );
    // The token metadata copy stays the same as the license record.
    let extra = c
        .nft_token(T1.into())
        .unwrap()
        .metadata
        .unwrap()
        .extra
        .unwrap();
    let v: near_sdk::serde_json::Value = near_sdk::serde_json::from_str(&extra).unwrap();
    assert_eq!(v["updates_until_ms"], l.updates_until_ms);
}

#[test]
fn renew_uses_the_price_period_and_payees_of_its_product() {
    let mut c = setup_two();
    call("bob.near", near(3), 10_000);
    c.buy(Q.into());
    call("bob.near", near(2), 20_000);
    let l = c.renew("doom:1".into());
    assert_eq!(l.updates_until_ms, 10_000 + 2 * 30 * DAY_MS);
    let paid = transfers();
    let fee = near(1).as_yoctonear() * 300 / 10_000;
    assert_eq!(amount_of(&paid, "fee.near"), fee);
    assert_eq!(amount_of(&paid, "dev2.near"), near(1).as_yoctonear() - fee);
    assert_eq!(amount_of(&paid, "dev.near"), 0);
    assert_eq!(
        amount_of(&paid, "bob.near"),
        near(1).as_yoctonear(),
        "excess"
    );
}

#[test]
#[should_panic(expected = "Only the owner can renew")]
fn renew_by_other_account_fails() {
    let mut c = with_license();
    call("bob.near", near(2), 20_000);
    c.renew(T1.into());
}

#[test]
#[should_panic(expected = "Attach at least")]
fn renew_with_too_small_deposit_fails() {
    let mut c = with_license();
    call("alice.near", near(1), 20_000);
    c.renew(T1.into());
}

#[test]
#[should_panic(expected = "No license has this token id")]
fn renew_of_unknown_token_fails() {
    let mut c = with_license();
    call("alice.near", near(2), 20_000);
    c.renew("checksum:99".into());
}

// ----- Transfers -----

#[test]
#[should_panic(expected = "change owner only through the market")]
fn nft_transfer_is_refused() {
    let mut c = with_license();
    call("alice.near", yocto(YOCTO), 20_000);
    c.nft_transfer(acc("bob.near"), T1.into(), None, None);
}

#[test]
#[should_panic(expected = "change owner only through the market")]
fn nft_transfer_call_is_refused() {
    let mut c = with_license();
    call("alice.near", yocto(YOCTO), 20_000);
    let _ = c.nft_transfer_call(acc("bob.near"), T1.into(), None, None, "x".into());
}

// ----- Market -----

#[test]
fn resale_moves_the_license_and_clears_the_listing() {
    let mut c = with_license();
    call("alice.near", yocto(YOCTO), 20_000);
    c.list(T1.into(), near(3));
    assert_eq!(c.get_listings(None, None, None).len(), 1);
    call("bob.near", near(3), 30_000);
    let l = c.buy_listed(T1.into(), near(3));
    assert_eq!(l.owner_id, acc("bob.near"));
    assert_eq!(l.listed_price, None);
    assert!(c.get_listings(None, None, None).is_empty());
    assert!(c.licenses_for_owner(acc("alice.near"), None).is_empty());
    // The license keeps its update period.
    assert_eq!(l.updates_until_ms, 10_000 + 365 * DAY_MS);
}

#[test]
fn resale_pays_the_fee_the_royalty_of_its_product_and_the_seller() {
    let mut c = setup_two();
    call("alice.near", near(5), 10_000);
    c.buy(P.into());
    call("alice.near", near(3), 10_000);
    c.buy(Q.into());
    let n = |m: u128| NearToken::from_millinear(m).as_yoctonear();

    // Product P: 3% fee, 10% royalty split 70/30, 87% to the seller.
    call("alice.near", yocto(YOCTO), 20_000);
    c.list(T1.into(), near(3));
    call("bob.near", near(4), 30_000);
    c.buy_listed(T1.into(), near(3));
    let paid = transfers();
    assert_eq!(amount_of(&paid, "fee.near"), n(90));
    assert_eq!(amount_of(&paid, "dev.near"), n(210));
    assert_eq!(amount_of(&paid, "contrib.near"), n(90));
    assert_eq!(amount_of(&paid, "alice.near"), n(2610));
    assert_eq!(amount_of(&paid, "dev2.near"), 0);
    assert_eq!(
        amount_of(&paid, "bob.near"),
        near(1).as_yoctonear(),
        "excess"
    );

    // Product Q: 3% fee, 5% royalty to dev2, 92% to the seller.
    call("alice.near", yocto(YOCTO), 20_000);
    c.list("doom:1".into(), near(2));
    call("bob.near", near(2), 30_000);
    c.buy_listed("doom:1".into(), near(2));
    let paid = transfers();
    assert_eq!(amount_of(&paid, "fee.near"), n(60));
    assert_eq!(amount_of(&paid, "dev2.near"), n(100));
    assert_eq!(amount_of(&paid, "alice.near"), n(1840));
    assert_eq!(amount_of(&paid, "dev.near"), 0);
    assert_eq!(amount_of(&paid, "contrib.near"), 0);
    assert_eq!(total(&paid), near(2).as_yoctonear());
}

#[test]
fn list_again_changes_the_price() {
    let mut c = with_license();
    call("alice.near", yocto(YOCTO), 20_000);
    c.list(T1.into(), near(3));
    c.list(T1.into(), near(4));
    assert_eq!(
        c.get_license(T1.into()).unwrap().listed_price,
        Some(near(4))
    );
}

#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn list_without_one_yocto_fails() {
    let mut c = with_license();
    call("alice.near", yocto(0), 20_000);
    c.list(T1.into(), near(3));
}

#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn delist_without_one_yocto_fails() {
    let mut c = with_license();
    call("alice.near", yocto(YOCTO), 20_000);
    c.list(T1.into(), near(3));
    call("alice.near", yocto(0), 20_000);
    c.delist(T1.into());
}

#[test]
#[should_panic(expected = "at least the renewal price")]
fn list_below_the_renewal_price_fails() {
    // A listing at a token price would skip the royalty.
    let mut c = with_license();
    call("alice.near", yocto(YOCTO), 20_000);
    c.list(T1.into(), yocto(1));
}

#[test]
fn list_at_the_renewal_price_is_allowed() {
    let mut c = with_license();
    call("alice.near", yocto(YOCTO), 20_000);
    c.list(T1.into(), near(2));
    assert_eq!(
        c.get_license(T1.into()).unwrap().listed_price,
        Some(near(2))
    );
}

#[test]
fn the_lowest_resale_price_is_the_renewal_price_of_the_product() {
    let mut c = setup_two();
    call("alice.near", near(5), 10_000);
    c.buy(P.into());
    call("alice.near", near(3), 10_000);
    c.buy(Q.into());
    // 1 NEAR is the renewal price of Q, and below the renewal price of P.
    call("alice.near", yocto(YOCTO), 20_000);
    c.list("doom:1".into(), near(1));
    let msg = panic_text(|| c.list(T1.into(), near(1)));
    assert!(msg.contains("at least the renewal price"), "{msg}");
}

#[test]
#[should_panic(expected = "Only the owner can list")]
fn list_by_other_account_fails() {
    let mut c = with_license();
    call("bob.near", yocto(YOCTO), 20_000);
    c.list(T1.into(), near(3));
}

#[test]
#[should_panic(expected = "The license is not listed")]
fn buy_listed_after_delist_fails() {
    let mut c = with_license();
    call("alice.near", yocto(YOCTO), 20_000);
    c.list(T1.into(), near(3));
    c.delist(T1.into());
    call("bob.near", near(3), 30_000);
    c.buy_listed(T1.into(), near(3));
}

#[test]
#[should_panic(expected = "The listed price changed")]
fn buy_listed_fails_when_the_seller_raised_the_price() {
    let mut c = with_license();
    call("alice.near", yocto(YOCTO), 20_000);
    c.list(T1.into(), near(3));
    c.list(T1.into(), near(9));
    // Bob saw 3 NEAR and attached 10.
    call("bob.near", near(10), 30_000);
    c.buy_listed(T1.into(), near(3));
}

#[test]
#[should_panic(expected = "Attach at least")]
fn buy_listed_with_too_small_deposit_fails() {
    let mut c = with_license();
    call("alice.near", yocto(YOCTO), 20_000);
    c.list(T1.into(), near(3));
    call("bob.near", near(2), 30_000);
    c.buy_listed(T1.into(), near(3));
}

#[test]
#[should_panic(expected = "cannot buy their own license")]
fn seller_cannot_buy_own_listing() {
    let mut c = with_license();
    call("alice.near", yocto(YOCTO), 20_000);
    c.list(T1.into(), near(3));
    call("alice.near", near(3), 30_000);
    c.buy_listed(T1.into(), near(3));
}

fn ids(list: Vec<LicenseView>) -> Vec<String> {
    let mut ids: Vec<String> = list.into_iter().map(|l| l.token_id).collect();
    ids.sort();
    ids
}

#[test]
fn listings_page_after_a_delist() {
    let mut c = setup();
    for who in ["a.near", "b.near", "c.near"] {
        call(who, near(5), 10_000);
        c.buy(P.into());
    }
    for (id, who) in [
        ("checksum:1", "a.near"),
        ("checksum:2", "b.near"),
        ("checksum:3", "c.near"),
    ] {
        call(who, yocto(YOCTO), 20_000);
        c.list(id.into(), near(3));
    }
    call("a.near", yocto(YOCTO), 20_000);
    c.delist("checksum:1".into());
    assert_eq!(
        ids(c.get_listings(None, None, None)),
        vec!["checksum:2", "checksum:3"]
    );
    assert_eq!(c.get_listings(None, Some(1), Some(1)).len(), 1);
    assert_eq!(c.get_listings(None, Some(2), None).len(), 0);
}

#[test]
fn get_listings_filters_by_product_before_the_page() {
    let mut c = setup_two();
    for (product, who, deposit) in [
        (Q, "a.near", near(3)),
        (P, "b.near", near(5)),
        (Q, "c.near", near(3)),
    ] {
        call(who, deposit, 10_000);
        c.buy(product.into());
    }
    for (id, who) in [
        ("doom:1", "a.near"),
        ("checksum:1", "b.near"),
        ("doom:2", "c.near"),
    ] {
        call(who, yocto(YOCTO), 20_000);
        c.list(id.into(), near(3));
    }
    assert_eq!(
        ids(c.get_listings(Some(P.into()), None, None)),
        vec!["checksum:1"]
    );
    assert_eq!(
        ids(c.get_listings(Some(Q.into()), None, None)),
        vec!["doom:1", "doom:2"]
    );
    assert_eq!(c.get_listings(None, None, None).len(), 3);
    // from_index counts the listings of the product only.
    let page = c.get_listings(Some(Q.into()), Some(1), Some(5));
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].product, Q);
    assert!(c.get_listings(Some(P.into()), Some(1), None).is_empty());
    assert!(c
        .get_listings(Some("nothing".into()), None, None)
        .is_empty());
}

#[test]
fn licenses_for_owner_filters_by_product() {
    let mut c = setup_two();
    call("alice.near", near(5), 10_000);
    c.buy(P.into());
    call("alice.near", near(3), 10_000);
    c.buy(Q.into());
    call("alice.near", near(3), 10_000);
    c.buy(Q.into());
    let alice = || acc("alice.near");
    assert_eq!(ids(c.licenses_for_owner(alice(), None)).len(), 3);
    assert_eq!(ids(c.licenses_for_owner(alice(), Some(P.into()))), vec![T1]);
    assert_eq!(
        ids(c.licenses_for_owner(alice(), Some(Q.into()))),
        vec!["doom:1", "doom:2"]
    );
    assert!(c
        .licenses_for_owner(alice(), Some("check".into()))
        .is_empty());
    assert!(c.licenses_for_owner(acc("bob.near"), None).is_empty());
}

// ----- Payment arithmetic -----

#[test]
fn sale_split_pays_fee_then_payees_and_adds_up() {
    let p = sale_split(near(5), 300, &acc("fee.near"), &terms().payees);
    assert_eq!(amount_of(&p, "fee.near"), near(5).as_yoctonear() * 3 / 100);
    let rest = near(5).as_yoctonear() * 97 / 100;
    assert_eq!(amount_of(&p, "contrib.near"), rest * 3 / 10);
    assert_eq!(amount_of(&p, "dev.near"), rest - rest * 3 / 10);
    assert_eq!(total(&p), near(5).as_yoctonear());
}

#[test]
fn resale_split_pays_fee_royalty_and_seller_and_adds_up() {
    let p = resale_split(
        near(3),
        300,
        &acc("fee.near"),
        1000,
        &terms().payees,
        &acc("alice.near"),
    );
    let n = NearToken::from_millinear;
    assert_eq!(amount_of(&p, "fee.near"), n(90).as_yoctonear());
    assert_eq!(amount_of(&p, "dev.near"), n(210).as_yoctonear());
    assert_eq!(amount_of(&p, "contrib.near"), n(90).as_yoctonear());
    assert_eq!(amount_of(&p, "alice.near"), n(2610).as_yoctonear());
    assert_eq!(total(&p), near(3).as_yoctonear());
}

#[test]
fn splits_add_up_for_odd_amounts_and_shares() {
    let payees = vec![
        Payee {
            account_id: acc("a.near"),
            bps: 3333,
        },
        Payee {
            account_id: acc("b.near"),
            bps: 3333,
        },
        Payee {
            account_id: acc("c.near"),
            bps: 3334,
        },
    ];
    for amount in [0u128, 1, 7, 999, 10_001, 123_456_789_012_345_678_901] {
        let a = NearToken::from_yoctonear(amount);
        assert_eq!(
            total(&sale_split(a, 300, &acc("fee.near"), &payees)),
            amount,
            "sale {amount}"
        );
        assert_eq!(
            total(&resale_split(
                a,
                300,
                &acc("fee.near"),
                1000,
                &payees,
                &acc("s.near")
            )),
            amount,
            "resale {amount}"
        );
    }
}

#[test]
fn bps_of_rounds_down() {
    assert_eq!(bps_of(yocto(999), 300), yocto(29));
    assert_eq!(bps_of(near(5), 300), NearToken::from_millinear(150));
}

// ----- Builds and entitlement -----

#[test]
fn entitlement_follows_release_time_and_owner() {
    let mut c = setup();
    call("dev.near", yocto(0), 5_000);
    c.add_build(P.into(), "0.1.0".into(), commit('a'), vec![artifact('a')]);
    call("alice.near", near(5), 10_000);
    c.buy(P.into());
    let ent = |c: &Contract, who: &str, v: &str| c.is_entitled(acc(who), P.into(), v.into());
    assert!(ent(&c, "alice.near", "0.1.0"), "build before the purchase");
    call("dev.near", yocto(0), 10_000 + 365 * DAY_MS);
    c.add_build(P.into(), "0.2.0".into(), commit('b'), vec![artifact('b')]);
    assert!(ent(&c, "alice.near", "0.2.0"), "build on the last day");
    call("dev.near", yocto(0), 10_001 + 365 * DAY_MS);
    c.add_build(P.into(), "0.3.0".into(), commit('c'), vec![artifact('c')]);
    assert!(!ent(&c, "alice.near", "0.3.0"), "build after the period");
    assert!(!ent(&c, "bob.near", "0.1.0"), "no license");
    assert!(!ent(&c, "alice.near", "9.9.9"), "unknown version");
    call("alice.near", yocto(YOCTO), 20_000);
    c.list(T1.into(), near(3));
    call("bob.near", near(3), 30_000);
    c.buy_listed(T1.into(), near(3));
    assert!(!ent(&c, "alice.near", "0.1.0"), "seller after resale");
    assert!(ent(&c, "bob.near", "0.1.0"), "buyer after resale");
}

#[test]
fn add_build_appends_in_the_first_day_and_find_artifact_finds_them() {
    let mut c = setup();
    call("dev.near", yocto(0), 5_000);
    c.add_build(P.into(), "0.1.0".into(), commit('a'), vec![artifact('a')]);
    call("dev.near", yocto(0), 5_000 + VERSION_OPEN_MS);
    let b = c.add_build(P.into(), "0.1.0".into(), commit('a'), vec![artifact('b')]);
    assert_eq!(b.artifacts.len(), 2);
    assert_eq!(
        b.released_at_ms, 5_000,
        "the release time is the first record"
    );
    let m = c.find_artifact(P.into(), sha('b').to_uppercase()).unwrap();
    assert_eq!(m.version, "0.1.0");
    assert_eq!(m.artifact.file, "app-b.deb");
    assert!(c.find_artifact(P.into(), sha('c')).is_none());
    assert_eq!(c.get_builds(P.into(), None, None).len(), 1);
    assert_eq!(
        c.get_build(P.into(), "0.1.0".into())
            .unwrap()
            .artifacts
            .len(),
        2
    );
}

#[test]
fn get_builds_lists_in_the_order_of_the_first_record_and_pages() {
    let mut c = setup();
    call("dev.near", yocto(0), 5_000);
    for (v, x) in [("0.2.0", 'a'), ("0.1.0", 'b'), ("1.0.0", 'c')] {
        c.add_build(P.into(), v.into(), commit(x), vec![artifact(x)]);
    }
    let versions =
        |list: Vec<Build>| -> Vec<String> { list.into_iter().map(|b| b.version).collect() };
    assert_eq!(
        versions(c.get_builds(P.into(), None, None)),
        vec!["0.2.0", "0.1.0", "1.0.0"]
    );
    assert_eq!(
        versions(c.get_builds(P.into(), Some(1), Some(1))),
        vec!["0.1.0"]
    );
    assert!(c.get_builds(P.into(), Some(3), None).is_empty());
}

#[test]
#[should_panic(expected = "This version is closed")]
fn add_build_refuses_artifacts_after_the_first_day() {
    let mut c = setup();
    call("dev.near", yocto(0), 5_000);
    c.add_build(P.into(), "0.1.0".into(), commit('a'), vec![artifact('a')]);
    call("dev.near", yocto(0), 5_001 + VERSION_OPEN_MS);
    c.add_build(P.into(), "0.1.0".into(), commit('a'), vec![artifact('b')]);
}

#[test]
#[should_panic(expected = "already recorded")]
fn add_build_refuses_a_recorded_artifact() {
    let mut c = setup();
    call("dev.near", yocto(0), 5_000);
    c.add_build(P.into(), "0.1.0".into(), commit('a'), vec![artifact('a')]);
    c.add_build(P.into(), "0.2.0".into(), commit('b'), vec![artifact('a')]);
}

#[test]
#[should_panic(expected = "already recorded")]
fn add_build_refuses_a_duplicate_in_one_call() {
    let mut c = setup();
    call("dev.near", yocto(0), 5_000);
    c.add_build(
        P.into(),
        "0.1.0".into(),
        commit('a'),
        vec![artifact('a'), artifact('a')],
    );
}

#[test]
#[should_panic(expected = "different commit")]
fn add_build_refuses_a_new_commit_for_a_version() {
    let mut c = setup();
    call("dev.near", yocto(0), 5_000);
    c.add_build(P.into(), "0.1.0".into(), commit('a'), vec![artifact('a')]);
    c.add_build(P.into(), "0.1.0".into(), commit('b'), vec![artifact('b')]);
}

#[test]
#[should_panic(expected = "Only the developer")]
fn add_build_by_other_account_fails() {
    let mut c = setup();
    call("alice.near", yocto(0), 5_000);
    c.add_build(P.into(), "0.1.0".into(), commit('a'), vec![artifact('a')]);
}

#[test]
#[should_panic(expected = "No product has this key")]
fn add_build_of_unknown_product_fails() {
    let mut c = setup();
    call("dev.near", yocto(0), 5_000);
    c.add_build(
        "nothing".into(),
        "0.1.0".into(),
        commit('a'),
        vec![artifact('a')],
    );
}

/// Each version and commit rule of add_build, with the panic text that it
/// must give. Every case fails before it writes, so one contract serves all.
#[test]
fn add_build_input_rules() {
    let mut c = setup();
    let cases: Vec<(String, String, Vec<Artifact>, &str)> = vec![
        (
            String::new(),
            commit('a'),
            vec![artifact('a')],
            "The version must have",
        ),
        (
            "v".repeat(65),
            commit('a'),
            vec![artifact('a')],
            "The version must have",
        ),
        (
            "1:0".into(),
            commit('a'),
            vec![artifact('a')],
            "cannot contain ':'",
        ),
        (
            "0.1.0".into(),
            "abc".into(),
            vec![artifact('a')],
            "The commit must be 40",
        ),
        (
            "0.1.0".into(),
            commit('A'),
            vec![artifact('a')],
            "The commit must be 40",
        ),
        (
            "0.1.0".into(),
            commit('a'),
            vec![],
            "Give at least one artifact",
        ),
    ];
    for (version, commit, artifacts, want) in cases {
        call("dev.near", yocto(0), 5_000);
        let msg = panic_text(|| {
            c.add_build(P.into(), version, commit, artifacts);
        });
        assert!(msg.contains(want), "{want}: {msg}");
    }
    call("dev.near", yocto(0), 5_000);
    c.add_build(P.into(), "v".repeat(64), commit('a'), vec![artifact('a')]);
}

#[test]
fn add_build_accepts_32_artifacts_and_refuses_33() {
    let many = |first: usize, n: usize| -> Vec<Artifact> {
        (first..first + n)
            .map(|i| Artifact {
                sha256: format!("{:064x}", i + 1),
                ..artifact('a')
            })
            .collect()
    };
    let mut c = setup();
    call("dev.near", yocto(0), 5_000);
    assert_eq!(
        c.add_build(P.into(), "0.1.0".into(), commit('a'), many(0, 32))
            .artifacts
            .len(),
        32
    );
    call("dev.near", yocto(0), 5_000);
    let msg = panic_text(|| {
        c.add_build(P.into(), "0.2.0".into(), commit('b'), many(100, 33));
    });
    assert!(msg.contains("Too many artifacts"), "{msg}");
}

#[test]
fn is_hex_checks_length_and_lowercase() {
    assert!(is_hex(&"0123456789abcdef".repeat(4), 64));
    assert!(!is_hex(&"0123456789ABCDEF".repeat(4), 64), "uppercase");
    assert!(!is_hex(&"a".repeat(63), 64), "63 characters");
    assert!(!is_hex(&"a".repeat(65), 64), "65 characters");
    assert!(!is_hex(&"g".repeat(64), 64), "not hex");
}

/// Each artifact check, with the panic text that it must give.
#[test]
fn artifact_checks() {
    let cases: Vec<(Artifact, &str)> = vec![
        (
            Artifact {
                sha256: "xyz".into(),
                ..artifact('a')
            },
            "64 lowercase hex",
        ),
        (
            Artifact {
                platform: String::new(),
                ..artifact('a')
            },
            "The platform must have",
        ),
        (
            Artifact {
                platform: "p".repeat(65),
                ..artifact('a')
            },
            "The platform must have",
        ),
        (
            Artifact {
                file: "f".repeat(256),
                ..artifact('a')
            },
            "The file name must have",
        ),
        (
            Artifact {
                attestation_url: Some("http://x".into()),
                ..artifact('a')
            },
            "must be https",
        ),
        (
            Artifact {
                attestation_url: Some(format!("https://{}", "x".repeat(510))),
                ..artifact('a')
            },
            "must be https",
        ),
    ];
    for (a, want) in cases {
        let msg = panic_text(|| check_artifact(&a));
        assert!(msg.contains(want), "{want}: got {msg}");
    }
    check_artifact(&Artifact {
        file: "f".repeat(255),
        ..artifact('a')
    });
}

// ----- Isolation between products -----

/// A call that changes a contract.
type ContractCall = Box<dyn Fn(&mut Contract)>;

#[test]
fn a_developer_cannot_change_another_product() {
    let mut c = setup_two();
    let one = || {
        vec![Payee {
            account_id: acc("dev2.near"),
            bps: 10_000,
        }]
    };
    let cases: Vec<(&str, ContractCall)> = vec![
        (
            "add_build",
            Box::new(|c: &mut Contract| {
                c.add_build(P.into(), "0.1.0".into(), commit('a'), vec![artifact('a')]);
            }),
        ),
        (
            "set_prices",
            Box::new(|c: &mut Contract| c.set_prices(P.into(), near(1), near(1))),
        ),
        (
            "set_payees",
            Box::new(move |c: &mut Contract| c.set_payees(P.into(), one())),
        ),
    ];
    for (what, f) in cases {
        call("dev2.near", yocto(0), 5_000);
        let msg = panic_text(|| f(&mut c));
        assert!(
            msg.contains("Only the developer of this product"),
            "{what}: {msg}"
        );
    }
    let v = c.get_product(P.into()).unwrap();
    assert_eq!((v.price, v.payees), (near(5), terms().payees));
    assert!(c.get_builds(P.into(), None, None).is_empty());
}

#[test]
fn two_products_can_record_the_same_artifact() {
    let mut c = setup_two();
    call("dev.near", yocto(0), 5_000);
    c.add_build(P.into(), "0.1.0".into(), commit('a'), vec![artifact('a')]);
    call("dev2.near", yocto(0), 6_000);
    c.add_build(Q.into(), "2.0".into(), commit('b'), vec![artifact('a')]);
    let in_p = c.find_artifact(P.into(), sha('a')).unwrap();
    let in_q = c.find_artifact(Q.into(), sha('a')).unwrap();
    assert_eq!((in_p.version.as_str(), in_p.commit), ("0.1.0", commit('a')));
    assert_eq!((in_q.version.as_str(), in_q.commit), ("2.0", commit('b')));
}

#[test]
fn build_views_see_only_their_product() {
    let mut c = setup_two();
    call("dev.near", yocto(0), 5_000);
    c.add_build(P.into(), "0.1.0".into(), commit('a'), vec![artifact('a')]);
    // Alice owns a license of Q only, bought after the build of P.
    call("alice.near", near(3), 10_000);
    c.buy(Q.into());
    call("alice.near", NearToken::from_millinear(10), 20_000);
    c.add_device("doom:1".into(), dev('a'));

    assert!(c.get_build(Q.into(), "0.1.0".into()).is_none());
    assert!(c.get_builds(Q.into(), None, None).is_empty());
    assert!(c.find_artifact(Q.into(), sha('a')).is_none());
    assert!(!c.is_entitled(acc("alice.near"), P.into(), "0.1.0".into()));
    assert!(!c.is_entitled(acc("alice.near"), Q.into(), "0.1.0".into()));
    let d = c
        .device_license(Q.into(), dev('a'), "0.1.0".into())
        .expect("seat in Q");
    assert!(!d.covers_version, "the version is a build of P");
    assert!(c
        .device_license(P.into(), dev('a'), "0.1.0".into())
        .is_none());
    assert_eq!(c.get_builds(P.into(), None, None).len(), 1);
}

#[test]
fn one_computer_can_have_a_seat_in_two_products() {
    let mut c = setup_two();
    call("alice.near", near(5), 10_000);
    c.buy(P.into());
    call("alice.near", near(3), 10_000);
    c.buy(Q.into());
    add(&mut c, T1, 'a');
    add(&mut c, "doom:1", 'a');
    let in_p = c.device_license(P.into(), dev('a'), "x".into()).unwrap();
    let in_q = c.device_license(Q.into(), dev('a'), "x".into()).unwrap();
    assert_eq!(in_p.token_id, T1);
    assert_eq!(in_q.token_id, "doom:1");
    call("alice.near", yocto(YOCTO), 20_000);
    c.remove_device(T1.into(), dev('a'));
    assert!(c.device_license(P.into(), dev('a'), "x".into()).is_none());
    assert_eq!(
        c.device_license(Q.into(), dev('a'), "x".into())
            .unwrap()
            .token_id,
        "doom:1"
    );
}

#[test]
fn seats_follow_max_devices_of_the_product() {
    let mut c = setup_two();
    call("alice.near", near(3), 10_000);
    c.buy(Q.into());
    add(&mut c, "doom:1", 'a');
    let msg = panic_text(|| {
        add(&mut c, "doom:1", 'b');
    });
    assert!(msg.contains("already has 1 computers"), "{msg}");
}

// ----- Computer seats -----

fn dev(c: char) -> String {
    c.to_string().repeat(64)
}

/// Registers a computer from alice's account with enough deposit for storage.
fn add(c: &mut Contract, token: &str, device: char) -> LicenseView {
    call("alice.near", NearToken::from_millinear(10), 20_000);
    c.add_device(token.into(), dev(device))
}

#[test]
fn owner_registers_up_to_three_computers() {
    let mut c = with_license();
    add(&mut c, T1, 'a');
    add(&mut c, T1, 'b');
    let l = add(&mut c, T1, 'c');
    assert_eq!(l.devices, vec![dev('a'), dev('b'), dev('c')]);
    let msg = panic_text(|| {
        add(&mut c, T1, 'd');
    });
    assert!(msg.contains("already has 3 computers"), "{msg}");
}

#[test]
fn device_license_reports_the_license_and_the_version() {
    let mut c = setup();
    call("dev.near", yocto(0), 5_000);
    c.add_build(P.into(), "0.1.0".into(), commit('a'), vec![artifact('a')]);
    call("alice.near", near(5), 10_000);
    c.buy(P.into());
    add(&mut c, T1, 'a');
    let d = c
        .device_license(P.into(), dev('a'), "0.1.0".into())
        .expect("seat");
    assert_eq!(
        (d.token_id.as_str(), d.owner_id.as_str()),
        (T1, "alice.near")
    );
    assert!(d.covers_version);
    let d = c
        .device_license(P.into(), dev('a'), "9.9.9".into())
        .expect("seat");
    assert!(!d.covers_version, "unknown version");
    assert!(
        c.device_license(P.into(), dev('b'), "0.1.0".into())
            .is_none(),
        "no seat"
    );
}

#[test]
fn remove_frees_a_seat() {
    let mut c = with_license();
    for d in ['a', 'b', 'c'] {
        add(&mut c, T1, d);
    }
    call("alice.near", yocto(YOCTO), 20_000);
    let l = c.remove_device(T1.into(), dev('b'));
    assert_eq!(l.devices, vec![dev('a'), dev('c')]);
    assert!(c.device_license(P.into(), dev('b'), "x".into()).is_none());
    add(&mut c, T1, 'd');
}

#[test]
fn resale_clears_the_seats() {
    let mut c = with_license();
    add(&mut c, T1, 'a');
    call("alice.near", yocto(YOCTO), 20_000);
    c.list(T1.into(), near(3));
    call("bob.near", near(3), 30_000);
    let l = c.buy_listed(T1.into(), near(3));
    assert!(l.devices.is_empty());
    assert!(
        c.device_license(P.into(), dev('a'), "x".into()).is_none(),
        "seller's computer"
    );
}

/// A change to a good add_device call, and the panic text that it must give.
type SeatCase = (&'static str, &'static str, String, NearToken, &'static str);

#[test]
fn seat_rules() {
    let ten = NearToken::from_millinear(10);
    let cases: Vec<SeatCase> = vec![
        ("bob.near", T1, dev('a'), ten, "Only the owner can register"),
        ("alice.near", T1, "xyz".into(), ten, "64 lowercase hex"),
        ("alice.near", T1, dev('a'), yocto(1), "Attach at least"),
        (
            "alice.near",
            "checksum:9",
            dev('a'),
            ten,
            "No license has this token id",
        ),
    ];
    // Every case fails before it changes the state, so one contract serves
    // all. (The mocked blockchain keeps storage between setups in one test.)
    let mut c = with_license();
    for (who, token, device, deposit, want) in cases {
        call(who, deposit, 20_000);
        let msg = panic_text(|| {
            c.add_device(token.into(), device);
        });
        assert!(msg.contains(want), "{want}: {msg}");
    }
}

#[test]
fn add_device_returns_the_excess_deposit() {
    let mut c = with_license();
    call("alice.near", NearToken::from_millinear(10), 20_000);
    let before = env::storage_usage();
    c.add_device(T1.into(), dev('a'));
    let cost = cost_since(before);
    assert!(cost > 0);
    assert_eq!(
        amount_of(&transfers(), "alice.near"),
        NearToken::from_millinear(10).as_yoctonear() - cost
    );
}

#[test]
fn a_computer_has_one_seat() {
    let mut c = setup();
    for _ in 0..2 {
        call("alice.near", near(5), 10_000);
        c.buy(P.into());
    }
    add(&mut c, T1, 'a');
    let msg = panic_text(|| {
        add(&mut c, T1, 'a');
    });
    assert!(msg.contains("already registered on this license"), "{msg}");
    let msg = panic_text(|| {
        add(&mut c, "checksum:2", 'a');
    });
    assert!(msg.contains("registered on another license"), "{msg}");
}

#[test]
fn remove_device_needs_one_yocto_and_a_known_computer() {
    let mut c = with_license();
    add(&mut c, T1, 'a');
    call("alice.near", yocto(0), 20_000);
    let msg = panic_text(|| {
        c.remove_device(T1.into(), dev('a'));
    });
    assert!(msg.contains("exactly 1 yoctoNEAR"), "{msg}");
    call("alice.near", yocto(YOCTO), 20_000);
    let msg = panic_text(|| {
        c.remove_device(T1.into(), dev('b'));
    });
    assert!(msg.contains("not on this license"), "{msg}");
    call("bob.near", yocto(YOCTO), 20_000);
    let msg = panic_text(|| {
        c.remove_device(T1.into(), dev('a'));
    });
    assert!(msg.contains("Only the owner can remove"), "{msg}");
}

#[test]
fn views_refuse_oversized_inputs() {
    let mut c = setup();
    call("dev.near", yocto(0), 5_000);
    c.add_build(P.into(), "0.1.0".into(), commit('a'), vec![artifact('a')]);
    call("alice.near", near(5), 10_000);
    c.buy(P.into());
    add(&mut c, T1, 'a');
    let long = "v".repeat(65);
    assert!(
        c.device_license(P.into(), dev('a'), long.clone()).is_none(),
        "long version"
    );
    assert!(
        !c.is_entitled(acc("alice.near"), P.into(), long),
        "long version"
    );
    assert!(
        c.device_license(P.into(), "x".repeat(10_000), "0.1.0".into())
            .is_none(),
        "bad device id"
    );
    assert!(
        c.device_license("x".repeat(10_000), dev('a'), "0.1.0".into())
            .is_none(),
        "bad product key"
    );
    assert!(
        c.find_artifact(P.into(), "x".repeat(10_000)).is_none(),
        "bad sha256"
    );
    assert!(
        c.device_license(P.into(), dev('a'), "0.1.0".into())
            .is_some(),
        "normal input"
    );
}
