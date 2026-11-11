//! BuildDeed license contract.
//!
//! One contract sells licenses for many products. A developer adds a product
//! with `add_product` and sets its terms: price, renewal price, update
//! period, royalty, payees and seats per license. Every license of every
//! product is a token of one NEP-171 collection, with the token id
//! `<product>:<number>`.
//!
//! A license gives its owner the official builds of its product that the
//! developer releases at or before `updates_until_ms`. The owner keeps those
//! builds after that time.
//!
//! The contract keeps the record of the official builds of each product: for
//! each version, the commit and the SHA-256 of each artifact. The record is
//! append-only. Records, seats and artifact hashes are kept per product, so
//! one developer cannot block or claim the files of another product.
//!
//! A license has up to `max_devices` computer seats. The owner registers a
//! computer with `add_device`; the program finds its license with
//! `device_license`. A resale clears the seats.
//!
//! A license changes owner only through the contract's market (`list` and
//! `buy_listed`), so that each resale pays the royalty on the listed price.
//! `nft_transfer` and `nft_transfer_call` are refused. A resale price is at
//! least the renewal price, so that a listing at a token price cannot skip
//! the royalty.
//!
//! The BuildDeed fee (`fee_bps` to `fee_account`) is set when the contract is
//! created. No method changes it, and no developer can remove it.

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
/// The highest BuildDeed fee that `new` accepts: 10%.
pub const MAX_FEE_BPS: u16 = 1_000;
const MAX_ARTIFACTS: usize = 32;
const MAX_URL: usize = 512;
const MAX_NAME: usize = 64;
const MAX_DESCRIPTION: usize = 280;
const MAX_FILE: usize = 255;
const MAX_PAGE: u32 = 100;
const MAX_PAYEES: usize = 16;
const MAX_DEVICES: u8 = 16;
const MAX_VERSIONS: usize = 1_000;

// The order of the variants is part of the stored state. Do not change it.
#[derive(BorshStorageKey)]
#[near]
enum StorageKey {
    OwnerById,
    TokenMetadata,
    Enumeration,
    ContractMetadata,
    Products,
    Licenses,
    Builds,
    Versions,
    ArtifactIndex,
    Listings,
    Devices,
    TokenDevices,
    Budgets,
}

/// A share of a sale.
#[near(serializers = [json, borsh])]
#[derive(Clone, Debug, PartialEq)]
pub struct Payee {
    pub account_id: AccountId,
    /// Share in basis points. The shares of all payees add up to 10 000.
    pub bps: u16,
}

/// The terms that a developer gives to `add_product`.
#[near(serializers = [json, borsh])]
#[derive(Clone, Debug)]
pub struct ProductTerms {
    /// The product name, for example "Iron Doom".
    pub name: String,
    /// One or two sentences for the website. At most 280 characters.
    pub description: String,
    /// The https URL of the source code or the product page.
    pub url: Option<String>,
    /// The price of a new license.
    pub price: NearToken,
    /// The price to add one `update_period_days` to a license. It is also the
    /// lowest resale price.
    pub renewal_price: NearToken,
    pub update_period_days: u32,
    /// The part of a resale price that goes to `payees`, in basis points.
    pub royalty_bps: u16,
    /// The developer and the contributors.
    pub payees: Vec<Payee>,
    /// The number of computers that one license can register.
    pub max_devices: u8,
}

/// One product, as stored.
#[near(serializers = [borsh])]
#[derive(Clone, Debug)]
pub struct Product {
    pub terms: ProductTerms,
    /// The account that added the product. It records builds and sets
    /// prices and payees.
    pub developer: AccountId,
    pub next_token: u64,
    /// NEAR that the developer paid for the storage of build records and
    /// term changes. `add_build` takes its storage from here, because a
    /// function-call key of a release pipeline cannot attach NEAR.
    pub storage_balance: NearToken,
}

/// One product, as the views return it.
#[near(serializers = [json])]
#[derive(Clone, Debug)]
pub struct ProductView {
    pub key: String,
    pub name: String,
    pub description: String,
    pub url: Option<String>,
    pub developer: AccountId,
    pub price: NearToken,
    pub renewal_price: NearToken,
    pub update_period_days: u32,
    pub royalty_bps: u16,
    pub payees: Vec<Payee>,
    pub max_devices: u8,
    pub fee_account: AccountId,
    pub fee_bps: u16,
    pub licenses_sold: u64,
    pub storage_balance: NearToken,
}

/// The settings of the contract. `new` sets them; no method changes them.
#[near(serializers = [json, borsh])]
#[derive(Clone, Debug)]
pub struct Settings {
    /// The BuildDeed fee account. It gets `fee_bps` of every payment.
    pub fee_account: AccountId,
    pub fee_bps: u16,
}

/// The state of one license.
#[near(serializers = [json, borsh])]
#[derive(Clone, Debug)]
pub struct License {
    pub purchased_at_ms: u64,
    /// The license covers each build released at or before this time.
    pub updates_until_ms: u64,
}

/// NEAR that an owner put in the contract for an agent, with limits. A
/// function-call key of the owner can buy licenses and register computers
/// from it (`buy_from_budget`, `add_device_from_budget`), because such a key
/// cannot attach NEAR. Only a full-access key can deposit, change the
/// limits or withdraw: those methods need an attached deposit.
#[near(serializers = [json, borsh])]
#[derive(Clone, Debug, PartialEq)]
pub struct Budget {
    pub balance: NearToken,
    /// The highest price of one license that the budget pays.
    pub max_price: NearToken,
    /// The products that the budget can buy. Empty: all products.
    pub products: Vec<String>,
}

#[near(serializers = [json])]
#[derive(Clone, Debug)]
pub struct LicenseView {
    pub token_id: TokenId,
    pub product: String,
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
    /// True if the license covers the version that the program asked about.
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
    /// The https URL to download the file, for example a GitHub release asset.
    pub url: Option<String>,
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
// without a state migration.
#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Contract {
    tokens: NonFungibleToken,
    metadata: LazyOption<NFTContractMetadata>,
    settings: Settings,
    products: IterableMap<String, Product>,
    licenses: LookupMap<TokenId, License>,
    /// "<product>:<version>" -> build
    builds: LookupMap<String, Build>,
    /// product -> versions, in the order of their first record
    versions: LookupMap<String, Vec<String>>,
    /// "<product>:<sha256>" -> version
    artifact_index: LookupMap<String, String>,
    listings: IterableMap<TokenId, NearToken>,
    /// "<product>:<device id>" -> token id
    devices: LookupMap<String, TokenId>,
    /// token id -> device ids
    token_devices: LookupMap<TokenId, Vec<String>>,
    /// owner -> agent budget
    budgets: LookupMap<AccountId, Budget>,
}

#[near]
impl Contract {
    #[init]
    pub fn new(fee_account: AccountId, fee_bps: u16, icon: Option<String>) -> Self {
        require!(fee_bps <= MAX_FEE_BPS, "The fee must be at most 10%");
        let metadata = NFTContractMetadata {
            spec: NFT_METADATA_SPEC.to_string(),
            name: "BuildDeed licenses".to_string(),
            symbol: "DEED".to_string(),
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
            settings: Settings {
                fee_account,
                fee_bps,
            },
            products: IterableMap::new(StorageKey::Products),
            licenses: LookupMap::new(StorageKey::Licenses),
            builds: LookupMap::new(StorageKey::Builds),
            versions: LookupMap::new(StorageKey::Versions),
            artifact_index: LookupMap::new(StorageKey::ArtifactIndex),
            listings: IterableMap::new(StorageKey::Listings),
            devices: LookupMap::new(StorageKey::Devices),
            token_devices: LookupMap::new(StorageKey::TokenDevices),
            budgets: LookupMap::new(StorageKey::Budgets),
        }
    }

    // ----- Products -----

    /// Adds a product. The caller becomes its developer. `key` is the short
    /// name in token ids and links: 2 to 32 characters `a-z`, `0-9` and `-`,
    /// starting with a letter or digit. Attach NEAR for storage: the product
    /// record takes about 0.01 NEAR, and the rest becomes the product's
    /// storage balance for build records (0.5 NEAR is enough for many
    /// releases).
    #[payable]
    pub fn add_product(&mut self, key: String, terms: ProductTerms) -> ProductView {
        require!(
            is_product_key(&key),
            "The product key must have 2 to 32 characters a-z, 0-9 and -, and start with a letter or digit"
        );
        require!(
            !self.products.contains_key(&key),
            "A product with this key exists"
        );
        self.check_terms(&terms);
        let developer = env::predecessor_account_id();
        let before = env::storage_usage();
        self.products.insert(
            key.clone(),
            Product {
                terms,
                developer: developer.clone(),
                next_token: 1,
                storage_balance: NearToken::from_yoctonear(0),
            },
        );
        self.products.flush();
        let cost = storage_cost_since(before);
        let rest = take_payment(cost);
        let p = self.products.get_mut(&key).expect("inserted above");
        p.storage_balance = rest;
        let event = json!({
            "standard": "builddeed",
            "version": "1.0.0",
            "event": "add_product",
            "data": [{ "product": key, "developer": developer }],
        });
        env::log_str(&format!("EVENT_JSON:{event}"));
        self.product_view(&key).expect("inserted above")
    }

    /// Adds NEAR to a product's storage balance. Anyone can pay.
    #[payable]
    pub fn add_storage(&mut self, product: String) -> NearToken {
        let p = self.product_mut(&product);
        p.storage_balance = p.storage_balance.saturating_add(env::attached_deposit());
        p.storage_balance
    }

    /// Changes the prices of a product for new sales and renewals.
    pub fn set_prices(&mut self, product: String, price: NearToken, renewal_price: NearToken) {
        self.require_developer(&product);
        let mut terms = self.product(&product).terms.clone();
        terms.price = price;
        terms.renewal_price = renewal_price;
        self.check_terms(&terms);
        self.update_terms(&product, terms);
    }

    /// Changes the developer and contributor shares of future sales.
    pub fn set_payees(&mut self, product: String, payees: Vec<Payee>) {
        self.require_developer(&product);
        let mut terms = self.product(&product).terms.clone();
        terms.payees = payees;
        self.check_terms(&terms);
        self.update_terms(&product, terms);
    }

    // ----- Sales -----

    /// Buys a new license of `product` for the caller. Attach at least the
    /// price; the contract returns the excess.
    #[payable]
    pub fn buy(&mut self, product: String) -> LicenseView {
        let buyer = env::predecessor_account_id();
        let excess = take_payment(self.product(&product).terms.price);
        let view = self.sell(&buyer, &product);
        pay(&buyer, excess);
        view
    }

    /// Buys a new license of `product` for the caller and pays it from the
    /// caller's agent budget. A function-call key can call it.
    pub fn buy_from_budget(&mut self, product: String) -> LicenseView {
        let buyer = env::predecessor_account_id();
        let price = self.product(&product).terms.price;
        let mut b = self.budget_of(&buyer);
        require!(
            b.products.is_empty() || b.products.contains(&product),
            "The agent budget does not allow this product"
        );
        require!(
            price <= b.max_price,
            "The price is more than the highest price of the agent budget"
        );
        require!(b.balance >= price, "The agent budget is too low");
        b.balance = b.balance.saturating_sub(price);
        self.budgets.insert(buyer.clone(), b);
        self.sell(&buyer, &product)
    }

    // ----- Agent budget -----

    /// Adds the attached NEAR to the caller's agent budget and sets its
    /// limits: the highest price of one license, and the products (empty:
    /// all). Attach at least 1 yoctoNEAR, so that a function-call key cannot
    /// change the limits. The storage of a new budget comes out of the
    /// deposit.
    #[payable]
    pub fn deposit_budget(&mut self, max_price: NearToken, products: Vec<String>) -> Budget {
        let owner = env::predecessor_account_id();
        let deposit = env::attached_deposit();
        require!(
            !deposit.is_zero(),
            "Attach at least 1 yoctoNEAR: only a full-access key can change a budget"
        );
        require!(products.len() <= MAX_PAYEES, "Give at most 16 products");
        for p in &products {
            require!(self.products.contains_key(p), "No product has this key");
        }
        let before = env::storage_usage();
        let mut b = self.budgets.get(&owner).cloned().unwrap_or(Budget {
            balance: NearToken::from_yoctonear(0),
            max_price,
            products: vec![],
        });
        b.max_price = max_price;
        b.products = products;
        self.budgets.insert(owner.clone(), b.clone());
        self.budgets.flush();
        let cost = storage_cost_since(before);
        require!(
            deposit >= cost,
            "The deposit does not cover the storage of the budget"
        );
        let b = self.budgets.get_mut(&owner).expect("inserted above");
        b.balance = b.balance.saturating_add(deposit.saturating_sub(cost));
        b.clone()
    }

    /// Pays `amount` of the caller's agent budget back to the caller, or all
    /// of it. Attach exactly 1 yoctoNEAR.
    #[payable]
    pub fn withdraw_budget(&mut self, amount: Option<NearToken>) -> Budget {
        assert_one_yocto();
        let owner = env::predecessor_account_id();
        let mut b = self.budget_of(&owner);
        let amount = amount.unwrap_or(b.balance);
        require!(amount <= b.balance, "The agent budget is too low");
        b.balance = b.balance.saturating_sub(amount);
        self.budgets.insert(owner.clone(), b.clone());
        pay(&owner, amount);
        b
    }

    // ----- Sales (internal) -----

    /// Mints a license of `product` to `buyer` and pays the price to the
    /// fee account and the payees. The caller has taken the price.
    fn sell(&mut self, buyer: &AccountId, product: &str) -> LicenseView {
        let p = self.product(product).clone();
        let price = p.terms.price;

        let token_id = format!("{product}:{}", p.next_token);
        self.product_mut(product).next_token += 1;
        let now = env::block_timestamp_ms();
        let license = License {
            purchased_at_ms: now,
            updates_until_ms: now + period_ms(&p.terms),
        };

        let storage_before = env::storage_usage();
        self.licenses.insert(token_id.clone(), license.clone());
        // store::LookupMap writes at flush time. Flush now, so that the
        // measurement below includes the license record.
        self.licenses.flush();
        let metadata = token_metadata(&p.terms.name, &token_id, &license);
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
            self.settings.fee_bps,
            &self.settings.fee_account,
            &p.terms.payees,
        ));
        let event = json!({
            "standard": "builddeed",
            "version": "1.0.0",
            "event": "buy",
            "data": [{ "token_id": token_id, "product": product, "owner_id": buyer, "price": price }],
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
        let p = self.product(product_of(&token_id)).clone();
        let price = p.terms.renewal_price;
        let excess = take_payment(price);
        let now = env::block_timestamp_ms();
        let mut license = self
            .licenses
            .get(&token_id)
            .expect("every token has a license record")
            .clone();
        license.updates_until_ms = license.updates_until_ms.max(now) + period_ms(&p.terms);
        self.licenses.insert(token_id.clone(), license.clone());
        let metadata = token_metadata(&p.terms.name, &token_id, &license);
        if let Some(map) = self.tokens.token_metadata_by_id.as_mut() {
            map.insert(&token_id, &metadata);
        }
        self.pay_all(sale_split(
            price,
            self.settings.fee_bps,
            &self.settings.fee_account,
            &p.terms.payees,
        ));
        pay(&caller, excess);
        self.license_view(&token_id).expect("the license exists")
    }

    /// Puts a license on the market for `price`, or changes its price. The
    /// price must be at least the renewal price. Attach exactly 1 yoctoNEAR,
    /// so that a function-call access key cannot list a license.
    #[payable]
    pub fn list(&mut self, token_id: TokenId, price: NearToken) {
        assert_one_yocto();
        require!(
            self.owner_of(&token_id) == env::predecessor_account_id(),
            "Only the owner can list a license"
        );
        let floor = self.product(product_of(&token_id)).terms.renewal_price;
        require!(
            price >= floor,
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
        let terms = self.product(product_of(&token_id)).terms.clone();

        self.listings.remove(&token_id);
        // The seller's computers lose the license; the buyer starts with no seats.
        self.clear_devices(&token_id);
        self.tokens
            .internal_transfer(&seller, &buyer, &token_id, None, Some("resale".to_string()));

        self.pay_all(resale_split(
            price,
            self.settings.fee_bps,
            &self.settings.fee_account,
            terms.royalty_bps,
            &terms.payees,
            &seller,
        ));
        pay(&buyer, excess);
        self.license_view(&token_id).expect("the license exists")
    }

    // ----- Computer seats -----

    /// Registers a computer on a license. Only the owner can do it, up to
    /// `max_devices` computers. `device_id` is the 64-hex id that the program
    /// shows. Attach NEAR for the storage (0.01 NEAR is enough); the contract
    /// returns the excess.
    #[payable]
    pub fn add_device(&mut self, token_id: TokenId, device_id: String) -> LicenseView {
        let caller = env::predecessor_account_id();
        let cost = self.seat(&caller, &token_id, device_id);
        let excess = take_payment(cost);
        pay(&caller, excess);
        self.license_view(&token_id).expect("the license exists")
    }

    /// Registers a computer on a license, as `add_device`, and pays the
    /// storage from the caller's agent budget. A function-call key can call
    /// it.
    pub fn add_device_from_budget(&mut self, token_id: TokenId, device_id: String) -> LicenseView {
        let caller = env::predecessor_account_id();
        let mut b = self.budget_of(&caller);
        let cost = self.seat(&caller, &token_id, device_id);
        require!(b.balance >= cost, "The agent budget is too low");
        b.balance = b.balance.saturating_sub(cost);
        self.budgets.insert(caller, b);
        self.license_view(&token_id).expect("the license exists")
    }

    /// Adds a seat and returns the cost of its storage. The caller pays it.
    fn seat(&mut self, caller: &AccountId, token_id: &TokenId, device_id: String) -> NearToken {
        require!(
            self.owner_of(token_id) == *caller,
            "Only the owner can register a computer"
        );
        require!(
            is_hex(&device_id, 64),
            "The device id must be 64 lowercase hex characters"
        );
        let product = product_of(token_id).to_string();
        let seat_key = format!("{product}:{device_id}");
        if let Some(other) = self.devices.get(&seat_key) {
            if other == token_id {
                env::panic_str("This computer is already registered on this license");
            }
            env::panic_str("This computer is registered on another license. Remove it there first");
        }
        let max = self.product(&product).terms.max_devices;
        let mut list = self
            .token_devices
            .get(token_id)
            .cloned()
            .unwrap_or_default();
        if list.len() >= usize::from(max) {
            env::panic_str(&format!(
                "This license already has {max} computers. Remove one first"
            ));
        }
        let storage_before = env::storage_usage();
        list.push(device_id);
        self.token_devices.insert(token_id.clone(), list);
        self.devices.insert(seat_key, token_id.clone());
        self.token_devices.flush();
        self.devices.flush();
        storage_cost_since(storage_before)
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
        self.devices
            .remove(&format!("{}:{device_id}", product_of(&token_id)));
        if list.is_empty() {
            self.token_devices.remove(&token_id);
        } else {
            self.token_devices.insert(token_id.clone(), list);
        }
        self.license_view(&token_id).expect("the license exists")
    }

    /// The license of `product` that a computer is registered on, and
    /// whether it covers `version`. None if the computer has no seat.
    pub fn device_license(
        &self,
        product: String,
        device_id: String,
        version: String,
    ) -> Option<DeviceLicense> {
        if !is_product_key(&product) || !is_hex(&device_id, 64) || version.len() > MAX_NAME {
            return None;
        }
        let token_id = self.devices.get(&format!("{product}:{device_id}"))?.clone();
        let owner_id = self.tokens.owner_by_id.get(&token_id)?;
        let l = self.licenses.get(&token_id)?;
        let covers_version = self
            .builds
            .get(&format!("{product}:{version}"))
            .is_some_and(|b| l.updates_until_ms >= b.released_at_ms);
        Some(DeviceLicense {
            token_id,
            owner_id,
            updates_until_ms: l.updates_until_ms,
            covers_version,
        })
    }

    // ----- Build records -----

    /// Records an official build of `product`, or adds artifacts to a
    /// version in the first 24 hours after its first record. A recorded
    /// artifact cannot be changed or removed. Only the developer can call
    /// it; the developer can give a function-call key that can call only
    /// this method to the release pipeline. The storage comes out of the
    /// product's storage balance (see `add_storage`).
    pub fn add_build(
        &mut self,
        product: String,
        version: String,
        commit: String,
        artifacts: Vec<Artifact>,
    ) -> Build {
        self.require_developer(&product);
        check_len(&version, MAX_NAME, "The version");
        require!(!version.contains(':'), "The version cannot contain ':'");
        require!(
            is_hex(&commit, 40),
            "The commit must be 40 lowercase hex characters"
        );
        require!(!artifacts.is_empty(), "Give at least one artifact");

        let now = env::block_timestamp_ms();
        let build_key = format!("{product}:{version}");
        let before = env::storage_usage();
        let mut build = match self.builds.get(&build_key) {
            Some(b) => {
                require!(b.commit == commit, "This version has a different commit");
                require!(
                    now <= b.released_at_ms + VERSION_OPEN_MS,
                    "This version is closed: record a new version"
                );
                b.clone()
            }
            None => {
                let mut list = self.versions.get(&product).cloned().unwrap_or_default();
                require!(
                    list.len() < MAX_VERSIONS,
                    "This product has too many versions"
                );
                list.push(version.clone());
                self.versions.insert(product.clone(), list);
                Build {
                    version: version.clone(),
                    commit,
                    released_at_ms: now,
                    artifacts: vec![],
                }
            }
        };
        for a in artifacts {
            check_artifact(&a);
            let index_key = format!("{product}:{}", a.sha256);
            if self.artifact_index.contains_key(&index_key) {
                env::panic_str(&format!("The artifact {} is already recorded", a.sha256));
            }
            self.artifact_index.insert(index_key, version.clone());
            build.artifacts.push(a);
        }
        require!(
            build.artifacts.len() <= MAX_ARTIFACTS,
            "Too many artifacts for one version"
        );
        self.builds.insert(build_key, build.clone());
        self.versions.flush();
        self.artifact_index.flush();
        self.builds.flush();
        self.charge_storage(&product, before);
        build
    }

    // ----- Views -----

    pub fn get_settings(&self) -> Settings {
        self.settings.clone()
    }

    pub fn get_product(&self, key: String) -> Option<ProductView> {
        self.product_view(&key)
    }

    /// Products in the order they were added.
    pub fn get_products(&self, from_index: Option<u32>, limit: Option<u32>) -> Vec<ProductView> {
        self.products
            .keys()
            .skip(from_index.unwrap_or(0) as usize)
            .take(page_limit(limit))
            .filter_map(|k| self.product_view(k))
            .collect()
    }

    pub fn get_license(&self, token_id: TokenId) -> Option<LicenseView> {
        self.license_view(&token_id)
    }

    /// The licenses of an account, of one product or of all products.
    pub fn licenses_for_owner(
        &self,
        account_id: AccountId,
        product: Option<String>,
    ) -> Vec<LicenseView> {
        self.token_ids_of(&account_id)
            .iter()
            .filter(|id| product.as_deref().is_none_or(|p| product_of(id) == p))
            .filter_map(|id| self.license_view(id))
            .collect()
    }

    /// Licenses on the market, of one product or of all products, in storage
    /// order. The order changes when a listing is removed. `from_index`
    /// counts the listings after the product filter.
    pub fn get_listings(
        &self,
        product: Option<String>,
        from_index: Option<u32>,
        limit: Option<u32>,
    ) -> Vec<LicenseView> {
        self.listings
            .keys()
            .filter(|id| product.as_deref().is_none_or(|p| product_of(id) == p))
            .skip(from_index.unwrap_or(0) as usize)
            .take(page_limit(limit))
            .filter_map(|id| self.license_view(id))
            .collect()
    }

    pub fn get_build(&self, product: String, version: String) -> Option<Build> {
        self.builds.get(&format!("{product}:{version}")).cloned()
    }

    /// The builds of a product in the order of their first record.
    pub fn get_builds(
        &self,
        product: String,
        from_index: Option<u32>,
        limit: Option<u32>,
    ) -> Vec<Build> {
        self.versions
            .get(&product)
            .map(|list| {
                list.iter()
                    .skip(from_index.unwrap_or(0) as usize)
                    .take(page_limit(limit))
                    .filter_map(|v| self.builds.get(&format!("{product}:{v}")).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The agent budget of an account, or None.
    pub fn get_budget(&self, account_id: AccountId) -> Option<Budget> {
        self.budgets.get(&account_id).cloned()
    }

    /// The newest build of `product` that has a file for `platform` (for
    /// example "linux-x86_64"), and that file. With more than one file for
    /// the platform, the first recorded one.
    pub fn latest_build(&self, product: String, platform: String) -> Option<ArtifactMatch> {
        let list = self.versions.get(&product)?;
        list.iter().rev().find_map(|v| {
            let b = self.builds.get(&format!("{product}:{v}"))?;
            let artifact = b.artifacts.iter().find(|a| a.platform == platform)?.clone();
            Some(ArtifactMatch {
                version: b.version.clone(),
                commit: b.commit.clone(),
                released_at_ms: b.released_at_ms,
                artifact,
            })
        })
    }

    /// Finds the official build of `product` that contains a file with this
    /// SHA-256.
    pub fn find_artifact(&self, product: String, sha256: String) -> Option<ArtifactMatch> {
        let sha256 = sha256.to_ascii_lowercase();
        if !is_hex(&sha256, 64) {
            return None;
        }
        let version = self.artifact_index.get(&format!("{product}:{sha256}"))?;
        let build = self.builds.get(&format!("{product}:{version}"))?;
        let artifact = build.artifacts.iter().find(|a| a.sha256 == sha256)?.clone();
        Some(ArtifactMatch {
            version: build.version.clone(),
            commit: build.commit.clone(),
            released_at_ms: build.released_at_ms,
            artifact,
        })
    }

    /// True if `account_id` owns a license of `product` that covers
    /// `version`: a license whose `updates_until_ms` is at or after the
    /// release time of the build.
    pub fn is_entitled(&self, account_id: AccountId, product: String, version: String) -> bool {
        if version.len() > MAX_NAME {
            return false;
        }
        let Some(build) = self.builds.get(&format!("{product}:{version}")) else {
            return false;
        };
        self.token_ids_of(&account_id)
            .iter()
            .filter(|id| product_of(id) == product)
            .any(|id| {
                self.licenses
                    .get(id)
                    .is_some_and(|l| l.updates_until_ms >= build.released_at_ms)
            })
    }

    // ----- Internal -----

    fn budget_of(&self, owner: &AccountId) -> Budget {
        self.budgets.get(owner).cloned().unwrap_or_else(|| {
            env::panic_str("This account has no agent budget. Call deposit_budget")
        })
    }

    fn product(&self, key: &str) -> &Product {
        self.products
            .get(key)
            .unwrap_or_else(|| env::panic_str("No product has this key"))
    }

    fn product_mut(&mut self, key: &str) -> &mut Product {
        self.products
            .get_mut(key)
            .unwrap_or_else(|| env::panic_str("No product has this key"))
    }

    fn require_developer(&self, product: &str) {
        require!(
            env::predecessor_account_id() == self.product(product).developer,
            "Only the developer of this product can do this"
        );
    }

    fn check_terms(&self, t: &ProductTerms) {
        check_terms(t, self.settings.fee_bps);
    }

    /// Stores new terms and charges a growth in storage to the product.
    fn update_terms(&mut self, product: &str, terms: ProductTerms) {
        let before = env::storage_usage();
        self.product_mut(product).terms = terms;
        self.products.flush();
        self.charge_storage(product, before);
    }

    /// Takes the storage used since `before` from the product's storage
    /// balance, or returns freed storage to it.
    fn charge_storage(&mut self, product: &str, before: u64) {
        let now = env::storage_usage();
        let byte = env::storage_byte_cost();
        let p = self.product_mut(product);
        if now >= before {
            let cost = byte.saturating_mul(u128::from(now - before));
            require!(
                p.storage_balance >= cost,
                "The storage balance of this product is too low. Call add_storage"
            );
            p.storage_balance = p.storage_balance.saturating_sub(cost);
        } else {
            let freed = byte.saturating_mul(u128::from(before - now));
            p.storage_balance = p.storage_balance.saturating_add(freed);
        }
    }

    fn product_view(&self, key: &str) -> Option<ProductView> {
        let p = self.products.get(key)?;
        let t = &p.terms;
        Some(ProductView {
            key: key.to_string(),
            name: t.name.clone(),
            description: t.description.clone(),
            url: t.url.clone(),
            developer: p.developer.clone(),
            price: t.price,
            renewal_price: t.renewal_price,
            update_period_days: t.update_period_days,
            royalty_bps: t.royalty_bps,
            payees: t.payees.clone(),
            max_devices: t.max_devices,
            fee_account: self.settings.fee_account.clone(),
            fee_bps: self.settings.fee_bps,
            licenses_sold: p.next_token - 1,
            storage_balance: p.storage_balance,
        })
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
            product: product_of(token_id).to_string(),
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
        let product = product_of(token_id).to_string();
        if let Some(list) = self.token_devices.remove(token_id) {
            for d in list {
                self.devices.remove(&format!("{product}:{d}"));
            }
        }
    }

    fn pay_all(&self, payments: Vec<(AccountId, u128)>) {
        for (account, amount) in payments {
            pay(&account, NearToken::from_yoctonear(amount));
        }
    }
}

/// The product key of a token id: "doom:42" -> "doom".
fn product_of(token_id: &str) -> &str {
    token_id.split_once(':').map_or(token_id, |(p, _)| p)
}

fn period_ms(t: &ProductTerms) -> u64 {
    u64::from(t.update_period_days) * MS_PER_DAY
}

/// The token metadata holds a copy of the license period for wallets.
/// `buy` and `renew` write both, so that they stay the same.
fn token_metadata(name: &str, token_id: &TokenId, l: &License) -> TokenMetadata {
    let extra = json!({
        "product": product_of(token_id),
        "purchased_at_ms": l.purchased_at_ms,
        "updates_until_ms": l.updates_until_ms,
    });
    let number = token_id
        .split_once(':')
        .map_or(token_id.as_str(), |(_, n)| n);
    TokenMetadata {
        title: Some(format!("{name} license #{number}")),
        description: Some(format!(
            "Gives the owner the official builds of {name} released at or before the end of the update period."
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

/// 2 to 32 characters a-z, 0-9 and -, starting with a letter or digit.
pub fn is_product_key(k: &str) -> bool {
    (2..=32).contains(&k.len())
        && k.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !k.starts_with('-')
}

fn check_terms(t: &ProductTerms, fee_bps: u16) {
    check_len(&t.name, MAX_NAME, "The product name");
    require!(
        t.description.len() <= MAX_DESCRIPTION,
        "The description must have at most 280 characters"
    );
    if let Some(url) = &t.url {
        require!(
            url.len() <= MAX_URL && url.starts_with("https://"),
            "The URL must be https and at most 512 characters"
        );
    }
    require!(!t.price.is_zero(), "The price must be more than zero");
    require!(
        !t.renewal_price.is_zero(),
        "The renewal price must be more than zero"
    );
    require!(
        t.update_period_days > 0 && t.update_period_days <= 36_500,
        "The update period must be 1 to 36500 days"
    );
    require!(
        u32::from(t.royalty_bps) + u32::from(fee_bps) <= BPS_TOTAL,
        "The royalty and the fee together must be at most 100%"
    );
    require!(
        !t.payees.is_empty() && t.payees.len() <= MAX_PAYEES,
        "Give 1 to 16 payees"
    );
    require!(
        t.max_devices >= 1 && t.max_devices <= MAX_DEVICES,
        "Give 1 to 16 computers for each license"
    );
    let mut accounts: Vec<&AccountId> = t.payees.iter().map(|p| &p.account_id).collect();
    accounts.sort();
    accounts.dedup();
    require!(
        accounts.len() == t.payees.len(),
        "Each payee account can appear only once"
    );
    let total: u32 = t.payees.iter().map(|p| u32::from(p.bps)).sum();
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
    if let Some(url) = &a.url {
        require!(
            url.len() <= MAX_URL && url.starts_with("https://"),
            "The download URL must be https and at most 512 characters"
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

#[cfg(test)]
mod tests;
