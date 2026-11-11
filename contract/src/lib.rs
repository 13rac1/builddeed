//! BuildDeed license contract.
//!
//! One contract sells licenses for one product. A license is a NEP-171 token.
//! It gives its owner the official builds that the developer releases at or
//! before `updates_until_ms`. The owner keeps those builds after that time.
//!
//! The contract also keeps the record of the official builds: for each version,
//! the commit and the SHA-256 of each artifact. The record is append-only.
//!
//! A license has up to `max_devices` computer seats. The owner registers a
//! computer with `add_device`; the app finds its license with
//! `device_license`. A resale clears the seats.
//!
//! A license changes owner only through the contract's market (`list` and
//! `buy_listed`), so that each resale pays the royalty on the listed price.
//! `nft_transfer` and `nft_transfer_call` are refused. A resale price is at
//! least `renewal_price`, so that a listing at a token price cannot skip the
//! royalty.

use near_contract_standards::non_fungible_token::core::NonFungibleTokenCore;
use near_contract_standards::non_fungible_token::enumeration::NonFungibleTokenEnumeration;
use near_contract_standards::non_fungible_token::metadata::{
    NFTContractMetadata, NonFungibleTokenMetadataProvider, TokenMetadata, NFT_METADATA_SPEC,
};
use near_contract_standards::non_fungible_token::{NonFungibleToken, Token, TokenId};
use near_sdk::collections::LazyOption;
use near_sdk::json_types::U128;
use near_sdk::serde_json::json;
use near_sdk::store::{IterableMap, LookupMap};
use near_sdk::{
    assert_one_yocto, env, near, require, AccountId, BorshStorageKey, NearToken, PanicOnDefault,
    Promise, PromiseOrValue,
};

/// 10 000 basis points = 100%.
pub const BPS_TOTAL: u32 = 10_000;
const MS_PER_DAY: u64 = 24 * 60 * 60 * 1000;
/// After this time from the first record of a version, the version is closed:
/// `add_build` refuses new artifacts for it.
pub const VERSION_OPEN_MS: u64 = MS_PER_DAY;
const MAX_ARTIFACTS: usize = 32;
const MAX_URL: usize = 512;
const MAX_NAME: usize = 64;
const MAX_FILE: usize = 255;
const MAX_PAGE: u32 = 100;
const MAX_PAYEES: usize = 16;
const MAX_DEVICES: u8 = 16;

// The order of the variants is part of the stored state. Do not change it.
#[derive(BorshStorageKey)]
#[near]
enum StorageKey {
    OwnerById,
    TokenMetadata,
    Enumeration,
    ContractMetadata,
    Licenses,
    Builds,
    ArtifactIndex,
    Listings,
    Devices,
    TokenDevices,
}

/// A share of a sale.
#[near(serializers = [json, borsh])]
#[derive(Clone, Debug, PartialEq)]
pub struct Payee {
    pub account_id: AccountId,
    /// Share in basis points. The shares of all payees add up to 10 000.
    pub bps: u16,
}

/// The sale terms of the product.
#[near(serializers = [json, borsh])]
#[derive(Clone, Debug)]
pub struct Config {
    /// The product name, for example "BuildDeed Checksum".
    pub product: String,
    /// The account that records builds and sets prices and payees.
    pub developer: AccountId,
    /// The price of a new license.
    pub price: NearToken,
    /// The price to add one `update_period_days` to a license. It is also the
    /// lowest resale price.
    pub renewal_price: NearToken,
    pub update_period_days: u32,
    /// The part of a resale price that goes to `payees`, in basis points.
    pub royalty_bps: u16,
    /// The BuildDeed fee account. It gets `fee_bps` of every payment.
    pub fee_account: AccountId,
    pub fee_bps: u16,
    /// The developer and the contributors.
    pub payees: Vec<Payee>,
    /// The number of computers that one license can register.
    pub max_devices: u8,
}

/// The state of one license.
#[near(serializers = [json, borsh])]
#[derive(Clone, Debug)]
pub struct License {
    pub purchased_at_ms: u64,
    /// The license covers each build released at or before this time.
    pub updates_until_ms: u64,
}

#[near(serializers = [json])]
#[derive(Clone, Debug)]
pub struct LicenseView {
    pub token_id: TokenId,
    pub owner_id: AccountId,
    pub purchased_at_ms: u64,
    pub updates_until_ms: u64,
    /// The resale price, if the license is on the market.
    pub listed_price: Option<NearToken>,
    /// The registered computers (device ids).
    pub devices: Vec<String>,
}

/// The result of `device_license`.
#[near(serializers = [json])]
#[derive(Clone, Debug)]
pub struct DeviceLicense {
    pub token_id: TokenId,
    pub owner_id: AccountId,
    pub updates_until_ms: u64,
    /// True if the license covers the version that the app asked about.
    pub covers_version: bool,
}

/// One file of a release.
#[near(serializers = [json, borsh])]
#[derive(Clone, Debug, PartialEq)]
pub struct Artifact {
    /// For example "linux-x86_64", "windows-x86_64", "macos-aarch64".
    pub platform: String,
    /// The file name, for example "builddeed-checksum_0.1.0_amd64.deb".
    pub file: String,
    /// SHA-256 of the file, 64 lowercase hex characters.
    pub sha256: String,
    /// The URL of the provenance attestation for this file.
    pub attestation_url: Option<String>,
}

/// One official release.
#[near(serializers = [json, borsh])]
#[derive(Clone, Debug)]
pub struct Build {
    pub version: String,
    /// The git commit, 40 lowercase hex characters.
    pub commit: String,
    /// Block time of the first `add_build` call for this version.
    pub released_at_ms: u64,
    pub artifacts: Vec<Artifact>,
}

/// The result of `find_artifact`.
#[near(serializers = [json])]
#[derive(Clone, Debug)]
pub struct ArtifactMatch {
    pub version: String,
    pub commit: String,
    pub released_at_ms: u64,
    pub artifact: Artifact,
}

// The fields and their order are the stored state. Do not change them
// without a state migration (see `migrate`).
#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Contract {
    tokens: NonFungibleToken,
    metadata: LazyOption<NFTContractMetadata>,
    config: Config,
    next_token_id: u64,
    licenses: LookupMap<TokenId, License>,
    builds: IterableMap<String, Build>,
    /// sha256 -> version
    artifact_index: LookupMap<String, String>,
    listings: IterableMap<TokenId, NearToken>,
    /// device id -> token id
    devices: LookupMap<String, TokenId>,
    /// token id -> device ids
    token_devices: LookupMap<TokenId, Vec<String>>,
}

#[near]
impl Contract {
    #[init]
    pub fn new(config: Config, symbol: String, icon: Option<String>) -> Self {
        check_config(&config);
        let metadata = NFTContractMetadata {
            spec: NFT_METADATA_SPEC.to_string(),
            name: format!("{} license", config.product),
            symbol,
            icon,
            base_uri: None,
            reference: None,
            reference_hash: None,
        };
        Self {
            tokens: NonFungibleToken::new(
                StorageKey::OwnerById,
                env::current_account_id(),
                Some(StorageKey::TokenMetadata),
                Some(StorageKey::Enumeration),
                None::<StorageKey>,
            ),
            metadata: LazyOption::new(StorageKey::ContractMetadata, Some(&metadata)),
            config,
            next_token_id: 1,
            licenses: LookupMap::new(StorageKey::Licenses),
            builds: IterableMap::new(StorageKey::Builds),
            artifact_index: LookupMap::new(StorageKey::ArtifactIndex),
            listings: IterableMap::new(StorageKey::Listings),
            devices: LookupMap::new(StorageKey::Devices),
            token_devices: LookupMap::new(StorageKey::TokenDevices),
        }
    }

    /// Upgrades the state of the first version (no computer seats) after a
    /// deploy of this code. Only the contract account can call it. The
    /// deploy and this call go in one transaction.
    #[private]
    #[init(ignore_state)]
    pub fn migrate(max_devices: u8) -> Self {
        let old: v1::Contract = env::state_read().expect("no state to migrate");
        let c = old.config;
        let config = Config {
            product: c.product,
            developer: c.developer,
            price: c.price,
            renewal_price: c.renewal_price,
            update_period_days: c.update_period_days,
            royalty_bps: c.royalty_bps,
            fee_account: c.fee_account,
            fee_bps: c.fee_bps,
            payees: c.payees,
            max_devices,
        };
        check_config(&config);
        Self {
            tokens: old.tokens,
            metadata: old.metadata,
            config,
            next_token_id: old.next_token_id,
            licenses: old.licenses,
            builds: old.builds,
            artifact_index: old.artifact_index,
            listings: old.listings,
            devices: LookupMap::new(StorageKey::Devices),
            token_devices: LookupMap::new(StorageKey::TokenDevices),
        }
    }

    // ----- Sales -----

    /// Buys a new license for the caller. Attach at least `price`; the
    /// contract returns the excess.
    #[payable]
    pub fn buy(&mut self) -> LicenseView {
        let buyer = env::predecessor_account_id();
        let price = self.config.price;
        let excess = take_payment(price);

        let token_id = self.next_token_id.to_string();
        self.next_token_id += 1;
        let now = env::block_timestamp_ms();
        let license = License {
            purchased_at_ms: now,
            updates_until_ms: now + self.period_ms(),
        };

        let storage_before = env::storage_usage();
        self.licenses.insert(token_id.clone(), license.clone());
        // store::LookupMap writes at flush time. Flush now, so that the
        // measurement below includes the license record.
        self.licenses.flush();
        let metadata = self.token_metadata(&token_id, &license);
        self.tokens.internal_mint_with_refund(
            token_id.clone(),
            buyer.clone(),
            Some(metadata),
            None,
        );
        let storage_cost = storage_cost_since(storage_before);
        require!(
            storage_cost < price,
            "The price does not cover the storage of a license"
        );

        // The storage of the new license comes out of the price, before the split.
        self.pay_all(sale_split(
            price.saturating_sub(storage_cost),
            self.config.fee_bps,
            &self.config.fee_account,
            &self.config.payees,
        ));
        pay(&buyer, excess);
        let event = json!({
            "standard": "builddeed",
            "version": "1.0.0",
            "event": "buy",
            "data": [{ "token_id": token_id, "owner_id": buyer, "price": price }],
        });
        env::log_str(&format!("EVENT_JSON:{event}"));
        self.license_view(&token_id)
            .expect("the license was minted above")
    }

    /// Adds one update period to a license. The period starts at the later of
    /// now and the current `updates_until_ms`. Only the owner can renew.
    #[payable]
    pub fn renew(&mut self, token_id: TokenId) -> LicenseView {
        let caller = env::predecessor_account_id();
        require!(
            self.owner_of(&token_id) == caller,
            "Only the owner can renew a license"
        );
        let price = self.config.renewal_price;
        let excess = take_payment(price);
        let now = env::block_timestamp_ms();
        let mut license = self
            .licenses
            .get(&token_id)
            .expect("every token has a license record")
            .clone();
        license.updates_until_ms = license.updates_until_ms.max(now) + self.period_ms();
        self.licenses.insert(token_id.clone(), license.clone());
        let metadata = self.token_metadata(&token_id, &license);
        if let Some(map) = self.tokens.token_metadata_by_id.as_mut() {
            map.insert(&token_id, &metadata);
        }
        self.pay_all(sale_split(
            price,
            self.config.fee_bps,
            &self.config.fee_account,
            &self.config.payees,
        ));
        pay(&caller, excess);
        self.license_view(&token_id).expect("the license exists")
    }

    /// Puts a license on the market for `price`, or changes its price. The
    /// price must be at least `renewal_price`. Attach exactly 1 yoctoNEAR, so
    /// that a function-call access key cannot list a license.
    #[payable]
    pub fn list(&mut self, token_id: TokenId, price: NearToken) {
        assert_one_yocto();
        require!(
            self.owner_of(&token_id) == env::predecessor_account_id(),
            "Only the owner can list a license"
        );
        require!(
            price >= self.config.renewal_price,
            "The resale price must be at least the renewal price"
        );
        self.listings.insert(token_id, price);
    }

    /// Takes a license off the market. Attach exactly 1 yoctoNEAR.
    #[payable]
    pub fn delist(&mut self, token_id: TokenId) {
        assert_one_yocto();
        require!(
            self.owner_of(&token_id) == env::predecessor_account_id(),
            "Only the owner can delist a license"
        );
        self.listings.remove(&token_id);
    }

    /// Buys a listed license. `price` is the price that the buyer saw; the
    /// call fails if the listed price is different now. Attach at least
    /// `price`; the contract returns the excess. The BuildDeed fee and the
    /// royalty go to their accounts, and the remainder goes to the seller.
    #[payable]
    pub fn buy_listed(&mut self, token_id: TokenId, price: NearToken) -> LicenseView {
        let buyer = env::predecessor_account_id();
        let listed = *self
            .listings
            .get(&token_id)
            .unwrap_or_else(|| env::panic_str("The license is not listed"));
        require!(
            listed == price,
            "The listed price changed. Look at the listing again"
        );
        let seller = self.owner_of(&token_id);
        require!(buyer != seller, "The owner cannot buy their own license");
        let excess = take_payment(price);

        self.listings.remove(&token_id);
        // The seller's computers lose the license; the buyer starts with no seats.
        self.clear_devices(&token_id);
        self.tokens
            .internal_transfer(&seller, &buyer, &token_id, None, Some("resale".to_string()));

        self.pay_all(resale_split(
            price,
            self.config.fee_bps,
            &self.config.fee_account,
            self.config.royalty_bps,
            &self.config.payees,
            &seller,
        ));
        pay(&buyer, excess);
        self.license_view(&token_id).expect("the license exists")
    }

    // ----- Computer seats -----

    /// Registers a computer on a license. Only the owner can do it, up to
    /// `max_devices` computers. `device_id` is the 64-hex id that the app
    /// shows. Attach NEAR for the storage (0.01 NEAR is enough); the contract
    /// returns the excess.
    #[payable]
    pub fn add_device(&mut self, token_id: TokenId, device_id: String) -> LicenseView {
        let caller = env::predecessor_account_id();
        require!(
            self.owner_of(&token_id) == caller,
            "Only the owner can register a computer"
        );
        require!(
            is_hex(&device_id, 64),
            "The device id must be 64 lowercase hex characters"
        );
        if let Some(other) = self.devices.get(&device_id) {
            if *other == token_id {
                env::panic_str("This computer is already registered on this license");
            }
            env::panic_str("This computer is registered on another license. Remove it there first");
        }
        let mut list = self
            .token_devices
            .get(&token_id)
            .cloned()
            .unwrap_or_default();
        if list.len() >= usize::from(self.config.max_devices) {
            env::panic_str(&format!(
                "This license already has {} computers. Remove one first",
                self.config.max_devices
            ));
        }
        let storage_before = env::storage_usage();
        list.push(device_id.clone());
        self.token_devices.insert(token_id.clone(), list);
        self.devices.insert(device_id, token_id.clone());
        self.token_devices.flush();
        self.devices.flush();
        let cost = storage_cost_since(storage_before);
        let excess = take_payment(cost);
        pay(&caller, excess);
        self.license_view(&token_id).expect("the license exists")
    }

    /// Removes a computer from a license, so that its seat is free again.
    /// Attach exactly 1 yoctoNEAR.
    #[payable]
    pub fn remove_device(&mut self, token_id: TokenId, device_id: String) -> LicenseView {
        assert_one_yocto();
        require!(
            self.owner_of(&token_id) == env::predecessor_account_id(),
            "Only the owner can remove a computer"
        );
        let mut list = self
            .token_devices
            .get(&token_id)
            .cloned()
            .unwrap_or_default();
        let before = list.len();
        list.retain(|d| *d != device_id);
        require!(list.len() < before, "This computer is not on this license");
        self.devices.remove(&device_id);
        if list.is_empty() {
            self.token_devices.remove(&token_id);
        } else {
            self.token_devices.insert(token_id.clone(), list);
        }
        self.license_view(&token_id).expect("the license exists")
    }

    /// The license that a computer is registered on, and whether it covers
    /// `version`. None if the computer has no seat.
    pub fn device_license(&self, device_id: String, version: String) -> Option<DeviceLicense> {
        if !is_hex(&device_id, 64) || version.len() > MAX_NAME {
            return None;
        }
        let token_id = self.devices.get(&device_id)?.clone();
        let owner_id = self.tokens.owner_by_id.get(&token_id)?;
        let l = self.licenses.get(&token_id)?;
        let covers_version = self
            .builds
            .get(&version)
            .is_some_and(|b| l.updates_until_ms >= b.released_at_ms);
        Some(DeviceLicense {
            token_id,
            owner_id,
            updates_until_ms: l.updates_until_ms,
            covers_version,
        })
    }

    // ----- Developer -----

    /// Records an official build, or adds artifacts to a version in the first
    /// 24 hours after its first record. A recorded artifact cannot be changed
    /// or removed. The developer can give a function-call key that can call
    /// only this method to the release pipeline.
    pub fn add_build(
        &mut self,
        version: String,
        commit: String,
        artifacts: Vec<Artifact>,
    ) -> Build {
        self.require_developer();
        check_len(&version, MAX_NAME, "The version");
        require!(
            is_hex(&commit, 40),
            "The commit must be 40 lowercase hex characters"
        );
        require!(!artifacts.is_empty(), "Give at least one artifact");

        let now = env::block_timestamp_ms();
        let mut build = match self.builds.get(&version) {
            Some(b) => {
                require!(b.commit == commit, "This version has a different commit");
                require!(
                    now <= b.released_at_ms + VERSION_OPEN_MS,
                    "This version is closed: record a new version"
                );
                b.clone()
            }
            None => Build {
                version: version.clone(),
                commit,
                released_at_ms: now,
                artifacts: vec![],
            },
        };
        for a in artifacts {
            check_artifact(&a);
            if self.artifact_index.contains_key(&a.sha256) {
                env::panic_str(&format!("The artifact {} is already recorded", a.sha256));
            }
            self.artifact_index
                .insert(a.sha256.clone(), version.clone());
            build.artifacts.push(a);
        }
        require!(
            build.artifacts.len() <= MAX_ARTIFACTS,
            "Too many artifacts for one version"
        );
        self.builds.insert(version, build.clone());
        build
    }

    /// Changes the prices for new sales and renewals.
    pub fn set_prices(&mut self, price: NearToken, renewal_price: NearToken) {
        self.require_developer();
        self.config.price = price;
        self.config.renewal_price = renewal_price;
        check_config(&self.config);
    }

    /// Changes the developer and contributor shares of future sales.
    pub fn set_payees(&mut self, payees: Vec<Payee>) {
        self.require_developer();
        self.config.payees = payees;
        check_config(&self.config);
    }

    // ----- Views -----

    pub fn get_config(&self) -> Config {
        self.config.clone()
    }

    pub fn get_license(&self, token_id: TokenId) -> Option<LicenseView> {
        self.license_view(&token_id)
    }

    pub fn licenses_for_owner(&self, account_id: AccountId) -> Vec<LicenseView> {
        self.token_ids_of(&account_id)
            .iter()
            .filter_map(|id| self.license_view(id))
            .collect()
    }

    /// Licenses on the market, in storage order. The order changes when a
    /// listing is removed.
    pub fn get_listings(&self, from_index: Option<u32>, limit: Option<u32>) -> Vec<LicenseView> {
        self.listings
            .keys()
            .skip(from_index.unwrap_or(0) as usize)
            .take(page_limit(limit))
            .filter_map(|id| self.license_view(id))
            .collect()
    }

    pub fn get_build(&self, version: String) -> Option<Build> {
        self.builds.get(&version).cloned()
    }

    /// Builds in storage order (the order of their first record).
    pub fn get_builds(&self, from_index: Option<u32>, limit: Option<u32>) -> Vec<Build> {
        self.builds
            .values()
            .skip(from_index.unwrap_or(0) as usize)
            .take(page_limit(limit))
            .cloned()
            .collect()
    }

    /// Finds the official build that contains a file with this SHA-256.
    pub fn find_artifact(&self, sha256: String) -> Option<ArtifactMatch> {
        let sha256 = sha256.to_ascii_lowercase();
        let version = self.artifact_index.get(&sha256)?;
        let build = self.builds.get(version)?;
        let artifact = build.artifacts.iter().find(|a| a.sha256 == sha256)?.clone();
        Some(ArtifactMatch {
            version: build.version.clone(),
            commit: build.commit.clone(),
            released_at_ms: build.released_at_ms,
            artifact,
        })
    }

    /// True if `account_id` owns a license that covers `version`: a license
    /// whose `updates_until_ms` is at or after the release time of the build.
    pub fn is_entitled(&self, account_id: AccountId, version: String) -> bool {
        if version.len() > MAX_NAME {
            return false;
        }
        let Some(build) = self.builds.get(&version) else {
            return false;
        };
        self.token_ids_of(&account_id).iter().any(|id| {
            self.licenses
                .get(id)
                .is_some_and(|l| l.updates_until_ms >= build.released_at_ms)
        })
    }

    // ----- Internal -----

    fn require_developer(&self) {
        require!(
            env::predecessor_account_id() == self.config.developer,
            "Only the developer can do this"
        );
    }

    fn period_ms(&self) -> u64 {
        u64::from(self.config.update_period_days) * MS_PER_DAY
    }

    fn owner_of(&self, token_id: &TokenId) -> AccountId {
        self.tokens
            .owner_by_id
            .get(token_id)
            .unwrap_or_else(|| env::panic_str("No license has this token id"))
    }

    fn token_ids_of(&self, account_id: &AccountId) -> Vec<TokenId> {
        self.tokens
            .tokens_per_owner
            .as_ref()
            .and_then(|m| m.get(account_id))
            .map(|set| set.iter().collect())
            .unwrap_or_default()
    }

    fn license_view(&self, token_id: &TokenId) -> Option<LicenseView> {
        let owner_id = self.tokens.owner_by_id.get(token_id)?;
        let l = self.licenses.get(token_id)?;
        Some(LicenseView {
            token_id: token_id.clone(),
            owner_id,
            purchased_at_ms: l.purchased_at_ms,
            updates_until_ms: l.updates_until_ms,
            listed_price: self.listings.get(token_id).copied(),
            devices: self
                .token_devices
                .get(token_id)
                .cloned()
                .unwrap_or_default(),
        })
    }

    fn clear_devices(&mut self, token_id: &TokenId) {
        if let Some(list) = self.token_devices.remove(token_id) {
            for d in list {
                self.devices.remove(&d);
            }
        }
    }

    /// The token metadata holds a copy of the license period for wallets.
    /// `buy` and `renew` write both, so that they stay the same.
    fn token_metadata(&self, token_id: &TokenId, l: &License) -> TokenMetadata {
        let extra = json!({
            "product": self.config.product,
            "purchased_at_ms": l.purchased_at_ms,
            "updates_until_ms": l.updates_until_ms,
        });
        TokenMetadata {
            title: Some(format!("{} license #{}", self.config.product, token_id)),
            description: Some(format!(
                "Gives the owner the official builds of {} released at or before the end of the update period.",
                self.config.product
            )),
            media: None,
            media_hash: None,
            copies: None,
            issued_at: Some(l.purchased_at_ms.to_string()),
            expires_at: None,
            starts_at: None,
            updated_at: Some(env::block_timestamp_ms().to_string()),
            extra: Some(extra.to_string()),
            reference: None,
            reference_hash: None,
        }
    }

    fn pay_all(&self, payments: Vec<(AccountId, u128)>) {
        for (account, amount) in payments {
            pay(&account, NearToken::from_yoctonear(amount));
        }
    }
}

// ----- Payment arithmetic -----

/// The payments of a first sale or a renewal: the BuildDeed fee, then the
/// rest split among the payees. The parts add up to `amount`.
pub fn sale_split(
    amount: NearToken,
    fee_bps: u16,
    fee_account: &AccountId,
    payees: &[Payee],
) -> Vec<(AccountId, u128)> {
    let fee = bps_of(amount, fee_bps);
    let mut out = vec![(fee_account.clone(), fee.as_yoctonear())];
    out.extend(split_among(amount.saturating_sub(fee), payees));
    out
}

/// The payments of a resale: the BuildDeed fee, the royalty split among the
/// payees, and the remainder to the seller. The parts add up to `price`.
pub fn resale_split(
    price: NearToken,
    fee_bps: u16,
    fee_account: &AccountId,
    royalty_bps: u16,
    payees: &[Payee],
    seller: &AccountId,
) -> Vec<(AccountId, u128)> {
    let fee = bps_of(price, fee_bps);
    let royalty = bps_of(price, royalty_bps);
    let mut out = vec![(fee_account.clone(), fee.as_yoctonear())];
    out.extend(split_among(royalty, payees));
    out.push((
        seller.clone(),
        price
            .saturating_sub(fee)
            .saturating_sub(royalty)
            .as_yoctonear(),
    ));
    out
}

/// Splits `amount` among the payees by their shares. The first payee also
/// gets the remainder of the integer division.
fn split_among(amount: NearToken, payees: &[Payee]) -> Vec<(AccountId, u128)> {
    let total = amount.as_yoctonear();
    let mut out: Vec<(AccountId, u128)> = Vec::with_capacity(payees.len());
    let mut paid = 0u128;
    for p in payees.iter().skip(1) {
        let share = total * u128::from(p.bps) / u128::from(BPS_TOTAL);
        paid += share;
        out.push((p.account_id.clone(), share));
    }
    if let Some(first) = payees.first() {
        out.insert(0, (first.account_id.clone(), total - paid));
    }
    out
}

// ----- NEP-171, NEP-177, NEP-181 -----

#[near]
impl NonFungibleTokenCore for Contract {
    /// Refused: a license changes owner only through `list` and `buy_listed`,
    /// so that each resale pays the royalty.
    #[payable]
    fn nft_transfer(
        &mut self,
        receiver_id: AccountId,
        token_id: TokenId,
        approval_id: Option<u64>,
        memo: Option<String>,
    ) {
        let _ = (receiver_id, token_id, approval_id, memo);
        env::panic_str(TRANSFER_REFUSED);
    }

    /// Refused, as `nft_transfer`.
    #[payable]
    fn nft_transfer_call(
        &mut self,
        receiver_id: AccountId,
        token_id: TokenId,
        approval_id: Option<u64>,
        memo: Option<String>,
        msg: String,
    ) -> PromiseOrValue<bool> {
        let _ = (receiver_id, token_id, approval_id, memo, msg);
        env::panic_str(TRANSFER_REFUSED);
    }

    fn nft_token(&self, token_id: TokenId) -> Option<Token> {
        self.tokens.nft_token(token_id)
    }
}

pub const TRANSFER_REFUSED: &str =
    "BuildDeed licenses change owner only through the market: call list, and the buyer calls buy_listed";

#[near]
impl NonFungibleTokenEnumeration for Contract {
    fn nft_total_supply(&self) -> U128 {
        self.tokens.nft_total_supply()
    }

    fn nft_tokens(&self, from_index: Option<U128>, limit: Option<u64>) -> Vec<Token> {
        self.tokens.nft_tokens(from_index, limit)
    }

    fn nft_supply_for_owner(&self, account_id: AccountId) -> U128 {
        self.tokens.nft_supply_for_owner(account_id)
    }

    fn nft_tokens_for_owner(
        &self,
        account_id: AccountId,
        from_index: Option<U128>,
        limit: Option<u64>,
    ) -> Vec<Token> {
        self.tokens
            .nft_tokens_for_owner(account_id, from_index, limit)
    }
}

#[near]
impl NonFungibleTokenMetadataProvider for Contract {
    fn nft_metadata(&self) -> NFTContractMetadata {
        self.metadata.get().expect("set in new")
    }
}

// ----- Checks and helpers -----

fn check_len(s: &str, max: usize, what: &str) {
    if s.is_empty() || s.len() > max {
        env::panic_str(&format!("{what} must have 1 to {max} characters"));
    }
}

fn check_config(c: &Config) {
    check_len(&c.product, MAX_NAME, "The product name");
    require!(!c.price.is_zero(), "The price must be more than zero");
    require!(
        !c.renewal_price.is_zero(),
        "The renewal price must be more than zero"
    );
    require!(
        c.update_period_days > 0,
        "The update period must be more than zero days"
    );
    require!(
        u32::from(c.royalty_bps) + u32::from(c.fee_bps) <= BPS_TOTAL,
        "The royalty and the fee together must be at most 100%"
    );
    require!(
        !c.payees.is_empty() && c.payees.len() <= MAX_PAYEES,
        "Give 1 to 16 payees"
    );
    require!(
        c.max_devices >= 1 && c.max_devices <= MAX_DEVICES,
        "Give 1 to 16 computers for each license"
    );
    let mut accounts: Vec<&AccountId> = c.payees.iter().map(|p| &p.account_id).collect();
    accounts.sort();
    accounts.dedup();
    require!(
        accounts.len() == c.payees.len(),
        "Each payee account can appear only once"
    );
    let total: u32 = c.payees.iter().map(|p| u32::from(p.bps)).sum();
    require!(
        total == BPS_TOTAL,
        "The payee shares must add up to 10000 basis points"
    );
}

fn check_artifact(a: &Artifact) {
    require!(
        is_hex(&a.sha256, 64),
        "The sha256 must be 64 lowercase hex characters"
    );
    check_len(&a.platform, MAX_NAME, "The platform");
    check_len(&a.file, MAX_FILE, "The file name");
    if let Some(url) = &a.attestation_url {
        require!(
            url.len() <= MAX_URL && url.starts_with("https://"),
            "The attestation URL must be https and at most 512 characters"
        );
    }
}

fn is_hex(s: &str, len: usize) -> bool {
    s.len() == len
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn page_limit(limit: Option<u32>) -> usize {
    limit.unwrap_or(MAX_PAGE).min(MAX_PAGE) as usize
}

/// Requires an attached deposit of at least `price` and returns the excess.
fn take_payment(price: NearToken) -> NearToken {
    let deposit = env::attached_deposit();
    if deposit < price {
        env::panic_str(&format!(
            "Attach at least {} yoctoNEAR",
            price.as_yoctonear()
        ));
    }
    deposit.saturating_sub(price)
}

fn bps_of(amount: NearToken, bps: u16) -> NearToken {
    NearToken::from_yoctonear(amount.as_yoctonear() * u128::from(bps) / u128::from(BPS_TOTAL))
}

fn storage_cost_since(before: u64) -> NearToken {
    let used = env::storage_usage().saturating_sub(before);
    env::storage_byte_cost().saturating_mul(u128::from(used))
}

fn pay(to: &AccountId, amount: NearToken) {
    if !amount.is_zero() {
        Promise::new(to.clone()).transfer(amount).detach();
    }
}

/// The stored state of the first version, for `migrate`.
mod v1 {
    use super::*;

    #[near(serializers = [borsh])]
    pub struct Config {
        pub product: String,
        pub developer: AccountId,
        pub price: NearToken,
        pub renewal_price: NearToken,
        pub update_period_days: u32,
        pub royalty_bps: u16,
        pub fee_account: AccountId,
        pub fee_bps: u16,
        pub payees: Vec<Payee>,
    }

    #[near(serializers = [borsh])]
    pub struct Contract {
        pub tokens: NonFungibleToken,
        pub metadata: LazyOption<NFTContractMetadata>,
        pub config: Config,
        pub next_token_id: u64,
        pub licenses: LookupMap<TokenId, License>,
        pub builds: IterableMap<String, Build>,
        pub artifact_index: LookupMap<String, String>,
        pub listings: IterableMap<TokenId, NearToken>,
    }
}

#[cfg(test)]
mod tests;
