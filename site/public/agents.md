# BuildDeed for agents

This page is for AI agents and the people who build them. It tells an agent
how to find a BuildDeed product, buy a license, register the computer it runs
on, and download and check the official build, with no browser.

## What BuildDeed is

BuildDeed sells software licenses as NFTs (NEP-171) on NEAR. The source code
of each product is public; a license gives the owner the verified official
build and its updates for an update period, on a fixed number of computers
("seats"). The program asks the contract at each start whether the computer
has a seat. Every license of every product is a token in one contract, the
official BuildDeed collection. The contract records the SHA-256 of each
official file, so anyone can check a download. An owner can resell a license
on the market; each sale pays the developers and contributors.

## Contract and network

| Item | Value |
| --- | --- |
| Network | NEAR testnet (testnet NEAR has no value) |
| Contract | `licenses.builddeed.testnet` |
| RPC | `https://rpc.testnet.fastnear.com` (JSON-RPC, a public shared service) |
| Amounts | yoctoNEAR as decimal strings: 1 NEAR = `"1000000000000000000000000"` |
| Token ids | `<product>:<n>`, for example `doom:1` |
| Protocol spec | <https://github.com/13rac1/builddeed-sdk/blob/main/spec/README.md> |

The RPC is shared with other people. Send one request at a time, and back off
on HTTP 429 or 503.

## Views

A view is free and needs no account. Send `query` with `call_function`; the
arguments are JSON, base64-encoded. For example, `get_product`:

```
curl -s https://rpc.testnet.fastnear.com -H 'Content-Type: application/json' -d '{
  "jsonrpc": "2.0", "id": "1", "method": "query",
  "params": {
    "request_type": "call_function", "finality": "optimistic",
    "account_id": "licenses.builddeed.testnet",
    "method_name": "get_product",
    "args_base64": "eyJrZXkiOiJkb29tIn0="
  }
}'
```

`eyJrZXkiOiJkb29tIn0=` is `{"key":"doom"}`. The reply has `result.result`, an
array of bytes: decode them as UTF-8 JSON.

With near-cli-rs:

```
near contract call-function as-read-only licenses.builddeed.testnet \
    get_product json-args '{"key":"doom"}' network-config testnet now
```

The replies below are examples; the numbers change.

### get_products

Args: `{"from_index": 0, "limit": 100}` (both optional; at most 100 for each
call). Returns a list of products in the order they were added, each as
`get_product` returns it.

### get_product

Args: `{"key": "doom"}`. Returns the product, or `null`.

```json
{
  "key": "doom",
  "name": "Iron Doom",
  "description": "A Doom engine in Rust, with Freedoom. Registered copies unlock level select.",
  "url": "https://github.com/ai-13rac1/iron-doom",
  "developer": "builddeed.testnet",
  "price": "5000000000000000000000000",
  "renewal_price": "2000000000000000000000000",
  "update_period_days": 365,
  "royalty_bps": 1000,
  "payees": [
    { "account_id": "builddeed.testnet", "bps": 7000 },
    { "account_id": "contrib.builddeed.testnet", "bps": 3000 }
  ],
  "max_devices": 3,
  "fee_account": "fee.builddeed.testnet",
  "fee_bps": 300,
  "licenses_sold": 0,
  "storage_balance": "495920000000000000000000"
}
```

`price` buys a new license. `max_devices` is the number of seats of one
license. `*_bps` values are basis points: 10000 = 100%.

### latest_build

Args: `{"product": "doom", "platform": "linux-x86_64"}`. Returns the newest
build that has a file for the platform, and that file, or `null`. Platforms
are `<os>-<arch>`, for example `linux-x86_64`, `windows-x86_64`,
`macos-aarch64`.

```json
{
  "version": "0.2.0",
  "commit": "0780757fdee7184e364f6145951506a794338536",
  "released_at_ms": 1762800000000,
  "artifact": {
    "platform": "linux-x86_64",
    "file": "iron-doom-0.2.0-linux-x86_64.tar.gz",
    "sha256": "3f1c…64 lowercase hex…9a0b",
    "attestation_url": "https://github.com/ai-13rac1/iron-doom/attestations/123456",
    "url": "https://github.com/ai-13rac1/iron-doom/releases/download/v0.2.0/iron-doom-0.2.0-linux-x86_64.tar.gz"
  }
}
```

`url` is the https download link, or `null` when the release did not record
one. `attestation_url` is the GitHub build provenance of the file, or `null`.

### find_artifact

Args: `{"product": "doom", "sha256": "<64 hex>"}`. Returns the official build
that contains a file with this SHA-256, in the same form as `latest_build`,
or `null`: then the file is not an official build of this product.

### device_license

Args: `{"product": "doom", "device_id": "<64 hex>", "version": "0.2.0"}`.
Returns the license that the computer has a seat on, or `null` (no seat).

```json
{
  "token_id": "doom:1",
  "owner_id": "alice.testnet",
  "updates_until_ms": 1825882492821,
  "covers_version": true
}
```

`covers_version` is true when the license covers the version: its
`updates_until_ms` is at or after the release time of that build. The program
runs registered only when the reply is not `null` and `covers_version` is
true.

### licenses_for_owner

Args: `{"account_id": "alice.testnet", "product": "doom"}` (`product` is
optional: without it, the licenses of all products).

```json
[
  {
    "token_id": "doom:1",
    "product": "doom",
    "owner_id": "alice.testnet",
    "purchased_at_ms": 1762800000000,
    "updates_until_ms": 1825882492821,
    "listed_price": null,
    "devices": ["6844b8b0…64 hex…af75"]
  }
]
```

`listed_price` is the resale price when the license is on the market.
`devices` are the computer ids that have a seat.

### get_budget

Args: `{"account_id": "alice.testnet"}`. Returns the agent budget of the
account, or `null`.

```json
{
  "balance": "9998000000000000000000000",
  "max_price": "5000000000000000000000000",
  "products": []
}
```

`max_price` is the highest price of one license that the budget pays.
`products` lists the products that the budget can buy; an empty list means
all products.

## Calls an agent makes

An agent signs with a function-call access key of the owner's account. Such a
key cannot attach NEAR, so these two methods pay from the owner's agent
budget. Both return the license, as in `licenses_for_owner`.

| Method | Args | What it does |
| --- | --- | --- |
| `buy_from_budget` | `{"product": "doom"}` | Buys a new license for the owner. The budget pays the price. It fails when the product is not in `products` (unless that list is empty), when the price is more than `max_price`, or when the balance is too low. |
| `add_device_from_budget` | `{"token_id": "doom:1", "device_id": "<64 hex>"}` | Gives a computer a seat on a license of the owner. The budget pays the storage of the seat (less than 0.01 NEAR). It fails when all seats are in use or the computer has a seat already. |

The key can call only these two methods: it cannot sell, list, renew or
transfer a license, remove a seat, deposit or withdraw, or change the limits.
Those methods also need an attached deposit, which a function-call key cannot
attach, so only a full-access key (the owner's wallet) can call them.

## Steps for the owner

1. **Deposit a budget.** On [My licenses](licenses.html#agent-budget),
   connect your wallet and deposit NEAR. Set the highest price of one license
   and, if you want, the products. This calls `deposit_budget` with
   `{"max_price": "<yocto>", "products": ["doom"]}` and the amount attached.
   The first deposit also pays the storage of the budget, about 0.002 NEAR.
   `withdraw_budget` with `{"amount": "<yocto>"}` or `{}` (all) and 1
   yoctoNEAR attached pays NEAR back.
2. **Give the agent a key.** With near-cli-rs, on a computer that has a
   full-access key of your account:

   ```
   near account add-key <account> grant-function-call-access \
       --allowance '0.25 NEAR' \
       --contract-account-id licenses.builddeed.testnet \
       --function-names buy_from_budget,add_device_from_budget \
       autogenerate-new-keypair save-to-legacy-keychain \
       network-config testnet sign-with-legacy-keychain send
   ```

   The allowance pays only gas. near-cli-rs saves the key in
   `~/.near-credentials/testnet/<account>/ed25519_<public-key>.json`. The
   `builddeed` CLI reads that file as it is and takes the account from the
   folder name. That file also holds a seed phrase, so to give the key to an
   agent on another computer, write a file with only the key:

   ```
   jq --arg a <account> '{account_id: $a, public_key, private_key}' \
       ~/.near-credentials/testnet/<account>/ed25519_<public-key>.json > key.json
   ```

   Give `key.json` to the agent. For a key file with no `account_id` in
   another folder, the agent adds `--account <account>`. To stop the agent, delete the key
   (`near account delete-keys`) or withdraw the budget.

## Steps for the agent

The `builddeed` CLI prints one JSON object for each command and exits with 0
on success. On a failure it prints `{"error": "..."}` and exits with 1.

1. **Install.**

   ```
   cargo install --git https://github.com/13rac1/builddeed-sdk builddeed --bin builddeed
   ```

2. **Buy** a license with the owner's key. This calls `buy_from_budget`.

   ```
   builddeed --product checksum buy --key key.json
   {"owner_id":"bd-agent-owner-1.testnet","token_id":"checksum:1"}
   ```

3. **Register** this computer. This calls `add_device_from_budget` on the
   given license, or on the first license of the product with a free seat.
   If this computer already has a seat on a license of the owner, it returns
   that license and sends nothing.

   ```
   builddeed --product checksum register --key key.json [--token checksum:1]
   {"device_id":"2504d5d35a8f4cacd36d095e293b79000d5d750c408b703c4beb8500a91c2ec4","token_id":"checksum:1"}
   ```

   There is no browser and no presence check here: the owner's own key signs
   on the computer that gets the seat.

4. **Download** the newest official file for this platform (default: the
   platform of this computer) and check it. See "What the download check
   proves".

   ```
   builddeed --product checksum download --platform linux-x86_64 --out dl
   {"attestation_url":"https://github.com/13rac1/builddeed/attestations/66185629","commit":"a4172143fcc5cbc4c5ea50e0a0334cdb43d61c42","path":"dl/builddeed-checksum-0.1.2-linux-x86_64","sha256":"6e0d63d93eb45b95f50359633a167823a31ff06ef804e86bfc5b4b1b07dd3430","version":"0.1.2"}
   ```

5. **Check** the seat. Without `--version`, it asks about the newest build
   for this platform.

   ```
   builddeed --product checksum status
   {"device_id":"2504d5d35a8f4cacd36d095e293b79000d5d750c408b703c4beb8500a91c2ec4","product":"checksum","status":{"owner_id":"bd-agent-owner-1.testnet","registered":true,"token_id":"checksum:1","updates_until_ms":1825949065608},"version":"0.1.2"}
   ```

   `"registered": false` comes with a `reason`: `"no seat"`, or `"the license
   ended before this version"`.

`builddeed --product doom device-id` prints the computer id. All commands
take `--contract` and `--rpc` to use another contract or RPC node.

## The computer id

A seat is for one computer id:

```
device_id = lowercase hex( SHA-256( "builddeed-device-v1" + "\n" + contract + "\n" + product + "\n" + machine_id ) )
```

- `machine_id` is the operating system's id of the installation, with
  surrounding white space removed. Linux: `/etc/machine-id`, then
  `/var/lib/dbus/machine-id`. macOS: `IOPlatformUUID` from `ioreg`. Windows:
  `MachineGuid` in `HKLM\SOFTWARE\Microsoft\Cryptography`.
- A machine id shorter than 16 characters, or `uninitialized`, is not usable:
  the computer has no id and the program runs unregistered.
- The program computes the id at each start and never stores it.
- Computers cloned from one disk image (virtual machines, containers) can
  share a machine id, and so one seat. A container without a usable
  `/etc/machine-id` or `/var/lib/dbus/machine-id` has no id.

The full rules are in the
[spec](https://github.com/13rac1/builddeed-sdk/blob/main/spec/README.md).

## What the download check proves

`builddeed download` takes the `url` of the newest record for the platform,
downloads the file over https, and compares its SHA-256 with the record in the
contract. If they are different, it deletes the file and fails.

A match proves:

- The file has the same bytes as the file that the product's release key
  recorded in the contract.
- The record has not changed since then: the contract cannot change or remove
  a recorded file.

A match does not prove:

- That the file is safe. The check says where the file came from, not what it
  does.
- That the release key was not stolen. A stolen key can record a false build
  for a new version.
- That the build came from the recorded commit. The CLI does not check the
  `attestation_url`. To check that, verify the GitHub attestation, or rebuild
  the recorded commit with the product's `scripts/verify.sh` and compare
  (Linux and Windows builds are reproducible).

To check a file that you have, call `find_artifact` with its SHA-256.

## Limits

- This is NEAR testnet. Testnet NEAR has no value, and the contract can be
  deployed again with a fresh state.
- The agent key can spend the whole budget, but only on `buy_from_budget`
  and `add_device_from_budget`, and only within the limits of the budget. It
  cannot sell, list, renew or transfer a license, withdraw, or change the
  limits.
- One budget for each account: all agent keys of an account share it.
- The key's allowance pays gas. When it is used up, the key stops working;
  add a new key.
- A license transfer outside the market is refused, so that each resale pays
  the royalty.
