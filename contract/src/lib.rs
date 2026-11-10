//! BuildDeed license contract.
//!
//! One contract sells licenses for one product. A license is a NEP-171 token.
//! It gives its owner the official builds that the developer releases before
//! `updates_until`. The owner keeps those builds after that time.
//!
//! The contract also keeps the record of the official builds: for each version,
//! the commit and the SHA-256 of each artifact. The record is append-only.
//!
//! A license changes owner only through the contract's market (`list` and
//! `buy_listed`), so that each resale pays the royalty. `nft_transfer` and
//! `nft_transfer_call` are refused.

use near_contract_standards::non_fungible_token::core::NonFungibleTokenCore;
use near_contract_standards::non_fungible_token::enumeration::NonFungibleTokenEnumeration;
use near_contract_standards::non_fungible_token::metadata::{
    NFTContractMetadata, NonFungibleTokenMetadataProvider, TokenMetadata, NFT_METADATA_SPEC,
};
use near_contract_standards::non_fungible_token::{NonFungibleToken, Token, TokenId};
use near_sdk::collections::LazyOption;
use near_sdk::json_types::U128;
use near_sdk::store::{IterableMap, LookupMap};
use near_sdk::{
    env, near, require, AccountId, BorshStorageKey, NearToken, PanicOnDefault, Promise,
    PromiseOrValue,
};

/// 10 000 basis points = 100%.
pub const BPS_TOTAL: u32 = 10_000;
const MS_PER_DAY: u64 = 24 * 60 * 60 * 1000;
const MAX_ARTIFACTS: usize = 32;
const MAX_TEXT: usize = 512;
const MAX_PAGE: u32 = 100;

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
    /// The price to add one `update_period_days` to a license.
    pub renewal_price: NearToken,
    pub update_period_days: u32,
    /// The part of a resale price that goes to `payees`, in basis points.
    pub royalty_bps: u16,
    /// The BuildDeed fee account. It gets `fee_bps` of every payment.
    pub fee_account: AccountId,
    pub fee_bps: u16,
    /// The developer and the contributors.
    pub payees: Vec<Payee>,
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
        let metadata = self.token_metadata(&token_id, &license);
        self.tokens
            .internal_mint_with_refund(token_id.clone(), buyer.clone(), Some(metadata), None);
        let storage_cost = storage_cost_since(storage_before);
        require!(
            storage_cost < price,
            "The price does not cover the storage of a license"
        );

        // The storage of the new token comes out of the price, before the split.
        self.pay_split(price.saturating_sub(storage_cost));
        refund(&buyer, excess);
        env::log_str(&format!(
            "EVENT_JSON:{{\"standard\":\"builddeed\",\"version\":\"1.0.0\",\"event\":\"buy\",\"data\":[{{\"token_id\":\"{}\",\"owner_id\":\"{}\",\"price\":\"{}\"}}]}}",
            token_id,
            buyer,
            price.as_yoctonear()
        ));
        self.license_view(&token_id).unwrap()
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
        let excess = take_payment(self.config.renewal_price);
        let period = self.period_ms();
        let now = env::block_timestamp_ms();
        let mut license = self.licenses.get(&token_id).unwrap().clone();
        license.updates_until_ms = license.updates_until_ms.max(now) + period;
        self.licenses.insert(token_id.clone(), license.clone());
        let metadata = self.token_metadata(&token_id, &license);
        if let Some(map) = self.tokens.token_metadata_by_id.as_mut() {
            map.insert(&token_id, &metadata);
        }
        self.pay_split(self.config.renewal_price);
        refund(&caller, excess);
        self.license_view(&token_id).unwrap()
    }

    /// Puts a license on the market for `price`, or changes its price.
    pub fn list(&mut self, token_id: TokenId, price: NearToken) {
        require!(
            self.owner_of(&token_id) == env::predecessor_account_id(),
            "Only the owner can list a license"
        );
        require!(!price.is_zero(), "The price must be more than zero");
        self.listings.insert(token_id, price);
    }

    /// Takes a license off the market.
    pub fn delist(&mut self, token_id: TokenId) {
        require!(
            self.owner_of(&token_id) == env::predecessor_account_id(),
            "Only the owner can delist a license"
        );
        self.listings.remove(&token_id);
    }

    /// Buys a listed license. Attach at least the listed price; the contract
    /// returns the excess. The BuildDeed fee and the royalty go to their
    /// accounts, and the remainder goes to the seller.
    #[payable]
    pub fn buy_listed(&mut self, token_id: TokenId) -> LicenseView {
        let buyer = env::predecessor_account_id();
        let price = *self
            .listings
            .get(&token_id)
            .unwrap_or_else(|| env::panic_str("The license is not listed"));
        let seller = self.owner_of(&token_id);
        require!(buyer != seller, "The owner cannot buy their own license");
        let excess = take_payment(price);

        self.listings.remove(&token_id);
        self.tokens
            .internal_transfer(&seller, &buyer, &token_id, None, Some("resale".to_string()));

        let royalty = bps_of(price, self.config.royalty_bps);
        let fee = bps_of(price, self.config.fee_bps);
        let to_seller = price.saturating_sub(royalty).saturating_sub(fee);
        pay(&self.config.fee_account, fee);
        self.pay_payees(royalty);
        pay(&seller, to_seller);
        refund(&buyer, excess);
        self.license_view(&token_id).unwrap()
    }

    // ----- Developer -----

    /// Records an official build, or adds artifacts to a recorded version.
    /// A recorded artifact cannot be changed or removed. The developer can
    /// give a function-call key that can call only this method to the
    /// release pipeline.
    pub fn add_build(&mut self, version: String, commit: String, artifacts: Vec<Artifact>) -> Build {
        self.require_developer();
        require!(
            !version.is_empty() && version.len() <= 64,
            "The version must have 1 to 64 characters"
        );
        require!(is_hex(&commit, 40), "The commit must be 40 lowercase hex characters");
        require!(!artifacts.is_empty(), "Give at least one artifact");

        let mut build = match self.builds.get(&version) {
            Some(b) => {
                require!(b.commit == commit, "This version has a different commit");
                b.clone()
            }
            None => Build {
                version: version.clone(),
                commit,
                released_at_ms: env::block_timestamp_ms(),
                artifacts: vec![],
            },
        };
        for a in artifacts {
            check_artifact(&a);
            require!(
                !self.artifact_index.contains_key(&a.sha256),
                format!("The artifact {} is already recorded", a.sha256)
            );
            self.artifact_index.insert(a.sha256.clone(), version.clone());
            build.artifacts.push(a);
        }
        require!(build.artifacts.len() <= MAX_ARTIFACTS, "Too many artifacts for one version");
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

    /// Licenses on the market, oldest listing first.
    pub fn get_listings(&self, from_index: Option<u32>, limit: Option<u32>) -> Vec<LicenseView> {
        self.listings
            .keys()
            .skip(from_index.unwrap_or(0) as usize)
            .take(limit.unwrap_or(MAX_PAGE).min(MAX_PAGE) as usize)
            .filter_map(|id| self.license_view(id))
            .collect()
    }

    pub fn get_build(&self, version: String) -> Option<Build> {
        self.builds.get(&version).cloned()
    }

    /// Builds in the order they were first recorded.
    pub fn get_builds(&self, from_index: Option<u32>, limit: Option<u32>) -> Vec<Build> {
        self.builds
            .values()
            .skip(from_index.unwrap_or(0) as usize)
            .take(limit.unwrap_or(MAX_PAGE).min(MAX_PAGE) as usize)
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
        })
    }

    fn token_metadata(&self, token_id: &TokenId, l: &License) -> TokenMetadata {
        TokenMetadata {
            title: Some(format!("{} license #{}", self.config.product, token_id)),
            description: Some(format!(
                "Gives the owner the official builds of {} released before the end of the update period.",
                self.config.product
            )),
            media: None,
            media_hash: None,
            copies: None,
            issued_at: Some(l.purchased_at_ms.to_string()),
            expires_at: None,
            starts_at: None,
            updated_at: Some(env::block_timestamp_ms().to_string()),
            extra: Some(format!(
                "{{\"product\":\"{}\",\"purchased_at_ms\":{},\"updates_until_ms\":{}}}",
                self.config.product.replace('"', ""),
                l.purchased_at_ms,
                l.updates_until_ms
            )),
            reference: None,
            reference_hash: None,
        }
    }

    /// Pays the BuildDeed fee from `amount`, then splits the rest among the
    /// payees.
    fn pay_split(&self, amount: NearToken) {
        let fee = bps_of(amount, self.config.fee_bps);
        pay(&self.config.fee_account, fee);
        self.pay_payees(amount.saturating_sub(fee));
    }

    /// Splits `amount` among the payees by their shares. The first payee
    /// also gets the remainder of the integer division.
    fn pay_payees(&self, amount: NearToken) {
        let mut paid = 0u128;
        let mut shares: Vec<(AccountId, u128)> = self
            .config
            .payees
            .iter()
            .map(|p| {
                let share = amount.as_yoctonear() * u128::from(p.bps) / u128::from(BPS_TOTAL);
                paid += share;
                (p.account_id.clone(), share)
            })
            .collect();
        shares[0].1 += amount.as_yoctonear() - paid;
        for (account, share) in shares {
            pay(&account, NearToken::from_yoctonear(share));
        }
    }
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

const TRANSFER_REFUSED: &str =
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
        self.tokens.nft_tokens_for_owner(account_id, from_index, limit)
    }
}

#[near]
impl NonFungibleTokenMetadataProvider for Contract {
    fn nft_metadata(&self) -> NFTContractMetadata {
        self.metadata.get().unwrap()
    }
}

// ----- Helpers -----

fn check_config(c: &Config) {
    require!(!c.product.is_empty() && c.product.len() <= 64, "The product name must have 1 to 64 characters");
    require!(!c.price.is_zero(), "The price must be more than zero");
    require!(c.update_period_days > 0, "The update period must be more than zero days");
    require!(
        u32::from(c.royalty_bps) + u32::from(c.fee_bps) <= BPS_TOTAL,
        "The royalty and the fee together must be at most 100%"
    );
    require!(!c.payees.is_empty() && c.payees.len() <= 16, "Give 1 to 16 payees");
    let total: u32 = c.payees.iter().map(|p| u32::from(p.bps)).sum();
    require!(total == BPS_TOTAL, "The payee shares must add up to 10000 basis points");
}

fn check_artifact(a: &Artifact) {
    require!(is_hex(&a.sha256, 64), "The sha256 must be 64 lowercase hex characters");
    require!(
        !a.platform.is_empty() && a.platform.len() <= 64,
        "The platform must have 1 to 64 characters"
    );
    require!(!a.file.is_empty() && a.file.len() <= 255, "The file name must have 1 to 255 characters");
    if let Some(url) = &a.attestation_url {
        require!(
            url.len() <= MAX_TEXT && url.starts_with("https://"),
            "The attestation URL must be https and at most 512 characters"
        );
    }
}

fn is_hex(s: &str, len: usize) -> bool {
    s.len() == len && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Requires an attached deposit of at least `price` and returns the excess.
fn take_payment(price: NearToken) -> NearToken {
    let deposit = env::attached_deposit();
    require!(
        deposit >= price,
        format!("Attach at least {} yoctoNEAR", price.as_yoctonear())
    );
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
        // near-sdk schedules the transfer when the Promise is dropped.
        let _ = Promise::new(to.clone()).transfer(amount);
    }
}

fn refund(to: &AccountId, amount: NearToken) {
    pay(to, amount);
}

#[cfg(test)]
mod tests;
