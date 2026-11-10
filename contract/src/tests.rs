use super::*;
use near_sdk::test_utils::VMContextBuilder;
use near_sdk::testing_env;

const DAY_MS: u64 = 24 * 60 * 60 * 1000;

fn acc(s: &str) -> AccountId {
    s.parse().unwrap()
}

fn near(n: u128) -> NearToken {
    NearToken::from_near(n)
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
            Payee { account_id: acc("dev.near"), bps: 7000 },
            Payee { account_id: acc("contrib.near"), bps: 3000 },
        ],
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
    call("dev.near", NearToken::from_yoctonear(0), 1_000);
    Contract::new(config(), "BDCK".into(), None)
}

fn sha(c: char) -> String {
    std::iter::repeat(c).take(64).collect()
}

fn commit(c: char) -> String {
    std::iter::repeat(c).take(40).collect()
}

fn artifact(c: char) -> Artifact {
    Artifact {
        platform: "linux-x86_64".into(),
        file: format!("app-{c}.deb"),
        sha256: sha(c),
        attestation_url: Some("https://github.com/13rac1/builddeed/attestations/1".into()),
    }
}

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
#[should_panic(expected = "Attach at least")]
fn buy_with_too_small_deposit_fails() {
    let mut c = setup();
    call("alice.near", near(4), 10_000);
    c.buy();
}

#[test]
fn renew_starts_from_updates_until_or_now() {
    let mut c = setup();
    call("alice.near", near(5), 10_000);
    c.buy();
    // Renew before the end: the period is added to updates_until.
    call("alice.near", near(2), 20_000);
    let l = c.renew("1".into());
    assert_eq!(l.updates_until_ms, 10_000 + 2 * 365 * DAY_MS);
    // Renew after the end: the period starts now.
    let late = 10_000 + 3 * 365 * DAY_MS;
    call("alice.near", near(2), late);
    let l = c.renew("1".into());
    assert_eq!(l.updates_until_ms, late + 365 * DAY_MS);
}

#[test]
#[should_panic(expected = "Only the owner can renew")]
fn renew_by_other_account_fails() {
    let mut c = setup();
    call("alice.near", near(5), 10_000);
    c.buy();
    call("bob.near", near(2), 20_000);
    c.renew("1".into());
}

#[test]
#[should_panic(expected = "change owner only through the market")]
fn nft_transfer_is_refused() {
    let mut c = setup();
    call("alice.near", near(5), 10_000);
    c.buy();
    call("alice.near", NearToken::from_yoctonear(1), 20_000);
    c.nft_transfer(acc("bob.near"), "1".into(), None, None);
}

#[test]
fn resale_moves_the_license_and_clears_the_listing() {
    let mut c = setup();
    call("alice.near", near(5), 10_000);
    c.buy();
    call("alice.near", NearToken::from_yoctonear(0), 20_000);
    c.list("1".into(), near(3));
    assert_eq!(c.get_listings(None, None).len(), 1);
    call("bob.near", near(3), 30_000);
    let l = c.buy_listed("1".into());
    assert_eq!(l.owner_id, acc("bob.near"));
    assert_eq!(l.listed_price, None);
    assert!(c.get_listings(None, None).is_empty());
    assert!(c.licenses_for_owner(acc("alice.near")).is_empty());
    // The license keeps its update period.
    assert_eq!(l.updates_until_ms, 10_000 + 365 * DAY_MS);
}

#[test]
#[should_panic(expected = "Only the owner can list")]
fn list_by_other_account_fails() {
    let mut c = setup();
    call("alice.near", near(5), 10_000);
    c.buy();
    call("bob.near", NearToken::from_yoctonear(0), 20_000);
    c.list("1".into(), near(3));
}

#[test]
#[should_panic(expected = "The license is not listed")]
fn buy_listed_after_delist_fails() {
    let mut c = setup();
    call("alice.near", near(5), 10_000);
    c.buy();
    call("alice.near", NearToken::from_yoctonear(0), 20_000);
    c.list("1".into(), near(3));
    c.delist("1".into());
    call("bob.near", near(3), 30_000);
    c.buy_listed("1".into());
}

#[test]
#[should_panic(expected = "cannot buy their own license")]
fn seller_cannot_buy_own_listing() {
    let mut c = setup();
    call("alice.near", near(5), 10_000);
    c.buy();
    call("alice.near", NearToken::from_yoctonear(0), 20_000);
    c.list("1".into(), near(3));
    call("alice.near", near(3), 30_000);
    c.buy_listed("1".into());
}

#[test]
fn entitlement_follows_release_time_and_owner() {
    let mut c = setup();
    call("dev.near", NearToken::from_yoctonear(0), 5_000);
    c.add_build("0.1.0".into(), commit('a'), vec![artifact('a')]);
    call("alice.near", near(5), 10_000);
    c.buy();
    // A build released before the purchase is covered.
    assert!(c.is_entitled(acc("alice.near"), "0.1.0".into()));
    // A build released inside the period is covered.
    call("dev.near", NearToken::from_yoctonear(0), 10_000 + 365 * DAY_MS);
    c.add_build("0.2.0".into(), commit('b'), vec![artifact('b')]);
    assert!(c.is_entitled(acc("alice.near"), "0.2.0".into()));
    // A build released after the period is not covered.
    call("dev.near", NearToken::from_yoctonear(0), 10_001 + 365 * DAY_MS);
    c.add_build("0.3.0".into(), commit('c'), vec![artifact('c')]);
    assert!(!c.is_entitled(acc("alice.near"), "0.3.0".into()));
    // No license, or an unknown version: not entitled.
    assert!(!c.is_entitled(acc("bob.near"), "0.1.0".into()));
    assert!(!c.is_entitled(acc("alice.near"), "9.9.9".into()));
    // After a resale, the entitlement moves to the buyer.
    call("alice.near", NearToken::from_yoctonear(0), 20_000);
    c.list("1".into(), near(3));
    call("bob.near", near(3), 30_000);
    c.buy_listed("1".into());
    assert!(!c.is_entitled(acc("alice.near"), "0.1.0".into()));
    assert!(c.is_entitled(acc("bob.near"), "0.1.0".into()));
}

#[test]
fn add_build_appends_artifacts_and_find_artifact_finds_them() {
    let mut c = setup();
    call("dev.near", NearToken::from_yoctonear(0), 5_000);
    c.add_build("0.1.0".into(), commit('a'), vec![artifact('a')]);
    call("dev.near", NearToken::from_yoctonear(0), 9_000);
    let b = c.add_build("0.1.0".into(), commit('a'), vec![artifact('b')]);
    assert_eq!(b.artifacts.len(), 2);
    // The release time is the time of the first call.
    assert_eq!(b.released_at_ms, 5_000);
    let m = c.find_artifact(sha('b').to_uppercase()).unwrap();
    assert_eq!(m.version, "0.1.0");
    assert_eq!(m.artifact.file, "app-b.deb");
    assert!(c.find_artifact(sha('c')).is_none());
    assert_eq!(c.get_builds(None, None).len(), 1);
}

#[test]
#[should_panic(expected = "already recorded")]
fn add_build_refuses_a_recorded_artifact() {
    let mut c = setup();
    call("dev.near", NearToken::from_yoctonear(0), 5_000);
    c.add_build("0.1.0".into(), commit('a'), vec![artifact('a')]);
    c.add_build("0.2.0".into(), commit('b'), vec![artifact('a')]);
}

#[test]
#[should_panic(expected = "different commit")]
fn add_build_refuses_a_new_commit_for_a_version() {
    let mut c = setup();
    call("dev.near", NearToken::from_yoctonear(0), 5_000);
    c.add_build("0.1.0".into(), commit('a'), vec![artifact('a')]);
    c.add_build("0.1.0".into(), commit('b'), vec![artifact('b')]);
}

#[test]
#[should_panic(expected = "Only the developer")]
fn add_build_by_other_account_fails() {
    let mut c = setup();
    call("alice.near", NearToken::from_yoctonear(0), 5_000);
    c.add_build("0.1.0".into(), commit('a'), vec![artifact('a')]);
}

#[test]
#[should_panic(expected = "64 lowercase hex")]
fn add_build_refuses_a_bad_hash() {
    let mut c = setup();
    call("dev.near", NearToken::from_yoctonear(0), 5_000);
    let mut a = artifact('a');
    a.sha256 = "xyz".into();
    c.add_build("0.1.0".into(), commit('a'), vec![a]);
}

#[test]
#[should_panic(expected = "add up to 10000")]
fn payee_shares_must_add_up() {
    let mut c = setup();
    call("dev.near", NearToken::from_yoctonear(0), 5_000);
    c.set_payees(vec![Payee { account_id: acc("dev.near"), bps: 9000 }]);
}

#[test]
fn bps_of_rounds_down() {
    assert_eq!(bps_of(NearToken::from_yoctonear(999), 300), NearToken::from_yoctonear(29));
    assert_eq!(bps_of(near(5), 300), NearToken::from_millinear(150));
}
