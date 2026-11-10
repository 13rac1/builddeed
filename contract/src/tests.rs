use super::*;
use near_sdk::test_utils::VMContextBuilder;
use near_sdk::testing_env;

const DAY_MS: u64 = 24 * 60 * 60 * 1000;
const YOCTO: u128 = 1;

fn acc(s: &str) -> AccountId {
    s.parse().unwrap()
}

fn near(n: u128) -> NearToken {
    NearToken::from_near(n)
}

fn yocto(n: u128) -> NearToken {
    NearToken::from_yoctonear(n)
}

fn config() -> Config {
    Config {
        product: "BuildDeed Checksum".into(),
        developer: acc("dev.near"),
        price: near(5),
        renewal_price: near(2),
        update_period_days: 365,
        royalty_bps: 1000,
        fee_account: acc("fee.near"),
        fee_bps: 300,
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

fn setup() -> Contract {
    call("dev.near", yocto(0), 1_000);
    Contract::new(config(), "BDCK".into(), None)
}

/// A contract with license "1" owned by alice, bought at 10 000 ms.
fn with_license() -> Contract {
    let mut c = setup();
    call("alice.near", near(5), 10_000);
    c.buy();
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

// ----- Buy and renew -----

#[test]
fn buy_mints_a_license_with_one_update_period() {
    let mut c = setup();
    call("alice.near", near(5), 10_000);
    let l = c.buy();
    assert_eq!(l.token_id, "1");
    assert_eq!(l.owner_id, acc("alice.near"));
    assert_eq!(l.purchased_at_ms, 10_000);
    assert_eq!(l.updates_until_ms, 10_000 + 365 * DAY_MS);
    assert_eq!(c.licenses_for_owner(acc("alice.near")).len(), 1);
    assert_eq!(c.nft_total_supply(), U128(1));
}

#[test]
fn token_metadata_extra_is_json_with_the_license_period() {
    let c = with_license();
    let token = c.nft_token("1".into()).expect("token 1");
    let extra = token.metadata.expect("metadata").extra.expect("extra");
    let v: near_sdk::serde_json::Value =
        near_sdk::serde_json::from_str(&extra).expect("valid JSON");
    assert_eq!(v["product"], "BuildDeed Checksum");
    assert_eq!(v["updates_until_ms"], 10_000 + 365 * DAY_MS);
}

#[test]
fn product_name_with_quote_and_backslash_gives_valid_json() {
    let mut cfg = config();
    cfg.product = "Odd \"name\\".into();
    call("dev.near", yocto(0), 1_000);
    let mut c = Contract::new(cfg, "X".into(), None);
    call("alice.near", near(5), 10_000);
    c.buy();
    let extra = c
        .nft_token("1".into())
        .unwrap()
        .metadata
        .unwrap()
        .extra
        .unwrap();
    let v: near_sdk::serde_json::Value =
        near_sdk::serde_json::from_str(&extra).expect("valid JSON");
    assert_eq!(v["product"], "Odd \"name\\");
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
    c.buy();
}

#[test]
fn renew_starts_from_updates_until_or_now_and_updates_the_metadata() {
    let mut c = with_license();
    // Renew before the end: the period is added to updates_until.
    call("alice.near", near(2), 20_000);
    let l = c.renew("1".into());
    assert_eq!(
        l.updates_until_ms,
        10_000 + 2 * 365 * DAY_MS,
        "renew before the end"
    );
    // Renew after the end: the period starts now.
    let late = 10_000 + 3 * 365 * DAY_MS;
    call("alice.near", near(2), late);
    let l = c.renew("1".into());
    assert_eq!(
        l.updates_until_ms,
        late + 365 * DAY_MS,
        "renew after the end"
    );
    // The token metadata copy stays the same as the license record.
    let extra = c
        .nft_token("1".into())
        .unwrap()
        .metadata
        .unwrap()
        .extra
        .unwrap();
    let v: near_sdk::serde_json::Value = near_sdk::serde_json::from_str(&extra).unwrap();
    assert_eq!(v["updates_until_ms"], l.updates_until_ms);
}

#[test]
#[should_panic(expected = "Only the owner can renew")]
fn renew_by_other_account_fails() {
    let mut c = with_license();
    call("bob.near", near(2), 20_000);
    c.renew("1".into());
}

#[test]
#[should_panic(expected = "Attach at least")]
fn renew_with_too_small_deposit_fails() {
    let mut c = with_license();
    call("alice.near", near(1), 20_000);
    c.renew("1".into());
}

#[test]
#[should_panic(expected = "No license has this token id")]
fn renew_of_unknown_token_fails() {
    let mut c = with_license();
    call("alice.near", near(2), 20_000);
    c.renew("99".into());
}

// ----- Transfers -----

#[test]
#[should_panic(expected = "change owner only through the market")]
fn nft_transfer_is_refused() {
    let mut c = with_license();
    call("alice.near", yocto(YOCTO), 20_000);
    c.nft_transfer(acc("bob.near"), "1".into(), None, None);
}

#[test]
#[should_panic(expected = "change owner only through the market")]
fn nft_transfer_call_is_refused() {
    let mut c = with_license();
    call("alice.near", yocto(YOCTO), 20_000);
    let _ = c.nft_transfer_call(acc("bob.near"), "1".into(), None, None, "x".into());
}

// ----- Market -----

#[test]
fn resale_moves_the_license_and_clears_the_listing() {
    let mut c = with_license();
    call("alice.near", yocto(YOCTO), 20_000);
    c.list("1".into(), near(3));
    assert_eq!(c.get_listings(None, None).len(), 1);
    call("bob.near", near(3), 30_000);
    let l = c.buy_listed("1".into(), near(3));
    assert_eq!(l.owner_id, acc("bob.near"));
    assert_eq!(l.listed_price, None);
    assert!(c.get_listings(None, None).is_empty());
    assert!(c.licenses_for_owner(acc("alice.near")).is_empty());
    // The license keeps its update period.
    assert_eq!(l.updates_until_ms, 10_000 + 365 * DAY_MS);
}

#[test]
fn list_again_changes_the_price() {
    let mut c = with_license();
    call("alice.near", yocto(YOCTO), 20_000);
    c.list("1".into(), near(3));
    c.list("1".into(), near(4));
    assert_eq!(
        c.get_license("1".into()).unwrap().listed_price,
        Some(near(4))
    );
}

#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn list_without_one_yocto_fails() {
    let mut c = with_license();
    call("alice.near", yocto(0), 20_000);
    c.list("1".into(), near(3));
}

#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn delist_without_one_yocto_fails() {
    let mut c = with_license();
    call("alice.near", yocto(YOCTO), 20_000);
    c.list("1".into(), near(3));
    call("alice.near", yocto(0), 20_000);
    c.delist("1".into());
}

#[test]
#[should_panic(expected = "at least the renewal price")]
fn list_below_the_renewal_price_fails() {
    // A listing at a token price would skip the royalty.
    let mut c = with_license();
    call("alice.near", yocto(YOCTO), 20_000);
    c.list("1".into(), yocto(1));
}

#[test]
fn list_at_the_renewal_price_is_allowed() {
    let mut c = with_license();
    call("alice.near", yocto(YOCTO), 20_000);
    c.list("1".into(), near(2));
    assert_eq!(
        c.get_license("1".into()).unwrap().listed_price,
        Some(near(2))
    );
}

#[test]
#[should_panic(expected = "Only the owner can list")]
fn list_by_other_account_fails() {
    let mut c = with_license();
    call("bob.near", yocto(YOCTO), 20_000);
    c.list("1".into(), near(3));
}

#[test]
#[should_panic(expected = "The license is not listed")]
fn buy_listed_after_delist_fails() {
    let mut c = with_license();
    call("alice.near", yocto(YOCTO), 20_000);
    c.list("1".into(), near(3));
    c.delist("1".into());
    call("bob.near", near(3), 30_000);
    c.buy_listed("1".into(), near(3));
}

#[test]
#[should_panic(expected = "The listed price changed")]
fn buy_listed_fails_when_the_seller_raised_the_price() {
    let mut c = with_license();
    call("alice.near", yocto(YOCTO), 20_000);
    c.list("1".into(), near(3));
    c.list("1".into(), near(9));
    // Bob saw 3 NEAR and attached 10.
    call("bob.near", near(10), 30_000);
    c.buy_listed("1".into(), near(3));
}

#[test]
#[should_panic(expected = "Attach at least")]
fn buy_listed_with_too_small_deposit_fails() {
    let mut c = with_license();
    call("alice.near", yocto(YOCTO), 20_000);
    c.list("1".into(), near(3));
    call("bob.near", near(2), 30_000);
    c.buy_listed("1".into(), near(3));
}

#[test]
#[should_panic(expected = "cannot buy their own license")]
fn seller_cannot_buy_own_listing() {
    let mut c = with_license();
    call("alice.near", yocto(YOCTO), 20_000);
    c.list("1".into(), near(3));
    call("alice.near", near(3), 30_000);
    c.buy_listed("1".into(), near(3));
}

#[test]
fn listings_page_after_a_delist() {
    let mut c = setup();
    for who in ["a.near", "b.near", "c.near"] {
        call(who, near(5), 10_000);
        c.buy();
    }
    for (id, who) in [("1", "a.near"), ("2", "b.near"), ("3", "c.near")] {
        call(who, yocto(YOCTO), 20_000);
        c.list(id.into(), near(3));
    }
    call("a.near", yocto(YOCTO), 20_000);
    c.delist("1".into());
    let mut ids: Vec<String> = c
        .get_listings(None, None)
        .into_iter()
        .map(|l| l.token_id)
        .collect();
    ids.sort();
    assert_eq!(ids, vec!["2", "3"]);
    assert_eq!(c.get_listings(Some(1), Some(1)).len(), 1);
    assert_eq!(c.get_listings(Some(2), None).len(), 0);
}

// ----- Payment arithmetic -----

#[test]
fn sale_split_pays_fee_then_payees_and_adds_up() {
    let p = sale_split(near(5), 300, &acc("fee.near"), &config().payees);
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
        &config().payees,
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
    c.add_build("0.1.0".into(), commit('a'), vec![artifact('a')]);
    call("alice.near", near(5), 10_000);
    c.buy();
    assert!(
        c.is_entitled(acc("alice.near"), "0.1.0".into()),
        "build before the purchase"
    );
    call("dev.near", yocto(0), 10_000 + 365 * DAY_MS);
    c.add_build("0.2.0".into(), commit('b'), vec![artifact('b')]);
    assert!(
        c.is_entitled(acc("alice.near"), "0.2.0".into()),
        "build on the last day"
    );
    call("dev.near", yocto(0), 10_001 + 365 * DAY_MS);
    c.add_build("0.3.0".into(), commit('c'), vec![artifact('c')]);
    assert!(
        !c.is_entitled(acc("alice.near"), "0.3.0".into()),
        "build after the period"
    );
    assert!(
        !c.is_entitled(acc("bob.near"), "0.1.0".into()),
        "no license"
    );
    assert!(
        !c.is_entitled(acc("alice.near"), "9.9.9".into()),
        "unknown version"
    );
    call("alice.near", yocto(YOCTO), 20_000);
    c.list("1".into(), near(3));
    call("bob.near", near(3), 30_000);
    c.buy_listed("1".into(), near(3));
    assert!(
        !c.is_entitled(acc("alice.near"), "0.1.0".into()),
        "seller after resale"
    );
    assert!(
        c.is_entitled(acc("bob.near"), "0.1.0".into()),
        "buyer after resale"
    );
}

#[test]
fn add_build_appends_in_the_first_day_and_find_artifact_finds_them() {
    let mut c = setup();
    call("dev.near", yocto(0), 5_000);
    c.add_build("0.1.0".into(), commit('a'), vec![artifact('a')]);
    call("dev.near", yocto(0), 5_000 + VERSION_OPEN_MS);
    let b = c.add_build("0.1.0".into(), commit('a'), vec![artifact('b')]);
    assert_eq!(b.artifacts.len(), 2);
    assert_eq!(
        b.released_at_ms, 5_000,
        "the release time is the first record"
    );
    let m = c.find_artifact(sha('b').to_uppercase()).unwrap();
    assert_eq!(m.version, "0.1.0");
    assert_eq!(m.artifact.file, "app-b.deb");
    assert!(c.find_artifact(sha('c')).is_none());
    assert_eq!(c.get_builds(None, None).len(), 1);
}

#[test]
#[should_panic(expected = "This version is closed")]
fn add_build_refuses_artifacts_after_the_first_day() {
    let mut c = setup();
    call("dev.near", yocto(0), 5_000);
    c.add_build("0.1.0".into(), commit('a'), vec![artifact('a')]);
    call("dev.near", yocto(0), 5_001 + VERSION_OPEN_MS);
    c.add_build("0.1.0".into(), commit('a'), vec![artifact('b')]);
}

#[test]
#[should_panic(expected = "already recorded")]
fn add_build_refuses_a_recorded_artifact() {
    let mut c = setup();
    call("dev.near", yocto(0), 5_000);
    c.add_build("0.1.0".into(), commit('a'), vec![artifact('a')]);
    c.add_build("0.2.0".into(), commit('b'), vec![artifact('a')]);
}

#[test]
#[should_panic(expected = "already recorded")]
fn add_build_refuses_a_duplicate_in_one_call() {
    let mut c = setup();
    call("dev.near", yocto(0), 5_000);
    c.add_build(
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
    c.add_build("0.1.0".into(), commit('a'), vec![artifact('a')]);
    c.add_build("0.1.0".into(), commit('b'), vec![artifact('b')]);
}

#[test]
#[should_panic(expected = "Only the developer")]
fn add_build_by_other_account_fails() {
    let mut c = setup();
    call("alice.near", yocto(0), 5_000);
    c.add_build("0.1.0".into(), commit('a'), vec![artifact('a')]);
}

#[test]
fn add_build_accepts_32_artifacts_and_refuses_33() {
    let hex = |i: usize| format!("{:064x}", i + 1);
    let many = |n: usize| -> Vec<Artifact> {
        (0..n)
            .map(|i| Artifact {
                sha256: hex(i),
                ..artifact('a')
            })
            .collect()
    };
    let mut c = setup();
    call("dev.near", yocto(0), 5_000);
    assert_eq!(
        c.add_build("0.1.0".into(), commit('a'), many(32))
            .artifacts
            .len(),
        32
    );
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut c = setup();
        call("dev.near", yocto(0), 5_000);
        c.add_build("0.1.0".into(), commit('a'), many(33));
    }));
    assert!(r.is_err(), "33 artifacts must fail");
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

// ----- Configuration -----

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

/// A change to a good configuration, and the panic text that it must give.
type ConfigCase = (fn(&mut Config), &'static str);

/// Each configuration rule, with the panic text that it must give.
#[test]
fn config_checks() {
    let cases: Vec<ConfigCase> = vec![
        (|c| c.product = String::new(), "The product name must have"),
        (|c| c.product = "p".repeat(65), "The product name must have"),
        (|c| c.price = yocto(0), "The price must be more than zero"),
        (
            |c| c.renewal_price = yocto(0),
            "The renewal price must be more than zero",
        ),
        (|c| c.update_period_days = 0, "update period"),
        (|c| c.royalty_bps = 9800, "at most 100%"),
        (|c| c.payees = vec![], "Give 1 to 16 payees"),
        (
            |c| {
                c.payees = (0..17)
                    .map(|i| Payee {
                        account_id: acc(&format!("p{i}.near")),
                        bps: 1,
                    })
                    .collect()
            },
            "Give 1 to 16 payees",
        ),
        (|c| c.payees[0].bps = 6999, "add up to 10000"),
        (|c| c.max_devices = 0, "Give 1 to 16 computers"),
        (|c| c.max_devices = 17, "Give 1 to 16 computers"),
    ];
    call("dev.near", yocto(0), 1_000);
    for (change, want) in cases {
        let mut cfg = config();
        change(&mut cfg);
        let msg = panic_text(|| check_config(&cfg));
        assert!(msg.contains(want), "{want}: got {msg:?}");
    }
    assert_eq!(panic_text(|| check_config(&config())), "");
}

#[test]
fn set_prices_and_set_payees_change_the_config() {
    let mut c = setup();
    call("dev.near", yocto(0), 5_000);
    c.set_prices(near(7), near(3));
    c.set_payees(vec![Payee {
        account_id: acc("dev.near"),
        bps: 10_000,
    }]);
    let cfg = c.get_config();
    assert_eq!((cfg.price, cfg.renewal_price), (near(7), near(3)));
    assert_eq!(cfg.payees.len(), 1);
}

#[test]
#[should_panic(expected = "The renewal price must be more than zero")]
fn set_prices_refuses_a_free_renewal() {
    let mut c = setup();
    call("dev.near", yocto(0), 5_000);
    c.set_prices(near(5), yocto(0));
}

#[test]
#[should_panic(expected = "Only the developer")]
fn set_prices_by_other_account_fails() {
    let mut c = setup();
    call("alice.near", yocto(0), 5_000);
    c.set_prices(near(1), near(1));
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
    add(&mut c, "1", 'a');
    add(&mut c, "1", 'b');
    let l = add(&mut c, "1", 'c');
    assert_eq!(l.devices, vec![dev('a'), dev('b'), dev('c')]);
    let msg = panic_text(|| {
        add(&mut c, "1", 'd');
    });
    assert!(msg.contains("already has 3 computers"), "{msg}");
}

#[test]
fn device_license_reports_the_license_and_the_version() {
    let mut c = setup();
    call("dev.near", yocto(0), 5_000);
    c.add_build("0.1.0".into(), commit('a'), vec![artifact('a')]);
    call("alice.near", near(5), 10_000);
    c.buy();
    add(&mut c, "1", 'a');
    let d = c.device_license(dev('a'), "0.1.0".into()).expect("seat");
    assert_eq!(
        (d.token_id.as_str(), d.owner_id.as_str()),
        ("1", "alice.near")
    );
    assert!(d.covers_version);
    let d = c.device_license(dev('a'), "9.9.9".into()).expect("seat");
    assert!(!d.covers_version, "unknown version");
    assert!(
        c.device_license(dev('b'), "0.1.0".into()).is_none(),
        "no seat"
    );
}

#[test]
fn remove_frees_a_seat() {
    let mut c = with_license();
    for d in ['a', 'b', 'c'] {
        add(&mut c, "1", d);
    }
    call("alice.near", yocto(YOCTO), 20_000);
    let l = c.remove_device("1".into(), dev('b'));
    assert_eq!(l.devices, vec![dev('a'), dev('c')]);
    assert!(c.device_license(dev('b'), "x".into()).is_none());
    add(&mut c, "1", 'd');
}

#[test]
fn resale_clears_the_seats() {
    let mut c = with_license();
    add(&mut c, "1", 'a');
    call("alice.near", yocto(YOCTO), 20_000);
    c.list("1".into(), near(3));
    call("bob.near", near(3), 30_000);
    let l = c.buy_listed("1".into(), near(3));
    assert!(l.devices.is_empty());
    assert!(
        c.device_license(dev('a'), "x".into()).is_none(),
        "seller's computer"
    );
}

/// A change to a good add_device call, and the panic text that it must give.
type SeatCase = (&'static str, &'static str, String, NearToken, &'static str);

#[test]
fn seat_rules() {
    let ten = NearToken::from_millinear(10);
    let cases: Vec<SeatCase> = vec![
        (
            "bob.near",
            "1",
            dev('a'),
            ten,
            "Only the owner can register",
        ),
        ("alice.near", "1", "xyz".into(), ten, "64 lowercase hex"),
        ("alice.near", "1", dev('a'), yocto(1), "Attach at least"),
        (
            "alice.near",
            "9",
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
fn a_computer_has_one_seat() {
    let mut c = setup();
    for _ in 0..2 {
        call("alice.near", near(5), 10_000);
        c.buy();
    }
    add(&mut c, "1", 'a');
    let msg = panic_text(|| {
        add(&mut c, "1", 'a');
    });
    assert!(msg.contains("already registered on this license"), "{msg}");
    let msg = panic_text(|| {
        add(&mut c, "2", 'a');
    });
    assert!(msg.contains("registered on another license"), "{msg}");
}

#[test]
fn remove_device_needs_one_yocto_and_a_known_computer() {
    let mut c = with_license();
    add(&mut c, "1", 'a');
    call("alice.near", yocto(0), 20_000);
    let msg = panic_text(|| {
        c.remove_device("1".into(), dev('a'));
    });
    assert!(msg.contains("exactly 1 yoctoNEAR"), "{msg}");
    call("alice.near", yocto(YOCTO), 20_000);
    let msg = panic_text(|| {
        c.remove_device("1".into(), dev('b'));
    });
    assert!(msg.contains("not on this license"), "{msg}");
}
