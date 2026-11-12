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
| Token ids | `<product>:<n>`, for example `checksum:1` |
| Products | `checksum` (BuildDeed Checksum) and `doom` (Room4Doom); `get_products` lists all |
| Website | <https://builddeed.dev/>; each product has a page at `https://builddeed.dev/p/<key>/` |
| Protocol spec | <https://github.com/13rac1/builddeed-sdk/blob/main/spec/README.md> |

The RPC is shared with other people. Send one request at a time, and back off
on HTTP 429 or 503.

## Views

A view is free and needs no account. Send `query` with `call_function`; the
arguments are JSON, base64-encoded. For example, `get_product`:

```sh
curl -s https://rpc.testnet.fastnear.com -H 'Content-Type: application/json' -d '{
  "jsonrpc": "2.0", "id": "1", "method": "query",
  "params": {
    "request_type": "call_function", "finality": "optimistic",
    "account_id": "licenses.builddeed.testnet",
    "method_name": "get_product",
    "args_base64": "eyJrZXkiOiJjaGVja3N1bSJ9"
  }
}'
```

`eyJrZXkiOiJjaGVja3N1bSJ9` is `{"key":"checksum"}`. The reply has
`result.result`, an array of bytes: decode them as UTF-8 JSON.

With near-cli-rs:

```sh
near contract call-function as-read-only licenses.builddeed.testnet \
    get_product json-args '{"key":"checksum"}' network-config testnet now
```

The replies below are examples. The prices are the testnet prices now: 0.1
NEAR for a license, 0.04 NEAR to renew. Read them again before you act: the
developer can change them.

### get_products

Args: `{"from_index": 0, "limit": 100}` (both optional; at most 100 for each
call). Returns a list of products in the order they were added, each as
`get_product` returns it.

### get_product

Args: `{"key": "checksum"}`. Returns the product, or `null`.

```json
{
  "key": "checksum",
  "name": "BuildDeed Checksum",
  "description": "Drop a file to see its SHA-256 and its official build record. Registered copies check whole folders.",
  "url": "https://github.com/13rac1/builddeed",
  "developer": "builddeed.testnet",
  "price": "100000000000000000000000",
  "renewal_price": "40000000000000000000000",
  "update_period_days": 365,
  "royalty_bps": 1000,
  "payees": [
    { "account_id": "builddeed.testnet", "bps": 7000 },
    { "account_id": "contrib.builddeed.testnet", "bps": 3000 }
  ],
  "max_devices": 3,
  "fee_account": "fee.builddeed.testnet",
  "fee_bps": 300,
  "licenses_sold": 1,
  "storage_balance": "495580000000000000000000"
}
```

`price` buys a new license; `buy_from_budget` needs this exact value.
`max_devices` is the number of seats of one license. `*_bps` values are basis
points: 10000 = 100%.

### latest_build

Args: `{"product": "checksum", "platform": "linux-x86_64"}`. Returns the last
recorded build that has a file for the platform, and that file, or `null`.
With more than one file for a platform, it returns the first recorded one:
a release records its main download for each platform first. Platforms are
`<os>-<arch>`: `linux-x86_64`, `windows-x86_64`, `macos-aarch64`.

```json
{
  "version": "0.1.2",
  "commit": "a4172143fcc5cbc4c5ea50e0a0334cdb43d61c42",
  "released_at_ms": 1762850000000,
  "artifact": {
    "platform": "linux-x86_64",
    "file": "builddeed-checksum-0.1.2-linux-x86_64",
    "sha256": "6e0d63d93eb45b95f50359633a167823a31ff06ef804e86bfc5b4b1b07dd3430",
    "attestation_url": "https://github.com/13rac1/builddeed/attestations/66185629",
    "url": "https://github.com/13rac1/builddeed/releases/download/v0.1.2/builddeed-checksum-0.1.2-linux-x86_64"
  }
}
```

`url` is the https download link, or `null` when the release did not record
one. `attestation_url` is the GitHub build provenance of the file, or `null`.

The file that each platform gets on testnet (BuildDeed Checksum 0.1.2):

| Platform | File | Then |
| --- | --- | --- |
| `linux-x86_64` | the program itself, `builddeed-checksum-<v>-linux-x86_64` | `chmod +x` it |
| `windows-x86_64` | the installer, `builddeed-checksum-<v>-windows-x86_64-setup.exe` | run it |
| `macos-aarch64` | the program itself, `builddeed-checksum-<v>-macos-aarch64` | `chmod +x` it |

Room4Doom has no recorded build yet: `latest_build` returns `null` for it.

### find_artifact

Args: `{"product": "checksum", "sha256": "<64 hex>"}`. Returns the official
build that contains a file with this SHA-256, in the same form as
`latest_build`, or `null`: then the file is not an official build of this
product.

### device_license

Args: `{"product": "checksum", "device_id": "<64 hex>", "version": "0.1.2"}`.
Returns the license that the computer has a seat on, or `null` (no seat).

```json
{
  "token_id": "checksum:1",
  "owner_id": "alice.testnet",
  "updates_until_ms": 1825949065608,
  "covers_version": true
}
```

`covers_version` is true when the license covers the version: its
`updates_until_ms` is at or after the release time of that build. The program
runs registered only when the reply is not `null` and `covers_version` is
true.

### licenses_for_owner

Args: `{"account_id": "alice.testnet", "product": "checksum"}` (`product` is
optional: without it, the licenses of all products).

```json
[
  {
    "token_id": "checksum:1",
    "product": "checksum",
    "owner_id": "alice.testnet",
    "purchased_at_ms": 1762850000000,
    "updates_until_ms": 1825949065608,
    "listed_price": null,
    "devices": ["2504d5d35a8f4cacd36d095e293b79000d5d750c408b703c4beb8500a91c2ec4"]
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
  "balance": "898000000000000000000000",
  "max_price": "100000000000000000000000",
  "products": ["checksum"]
}
```

`max_price` is the highest price of one license that the budget pays.
`products` lists the 1 to 16 products that the budget can buy and register.

## Calls an agent makes

An agent signs with a function-call access key of the owner's account. Such a
key cannot attach NEAR, so these two methods pay from the owner's agent
budget. Both return the license, as in `licenses_for_owner`.

| Method | Args | What it does |
| --- | --- | --- |
| `buy_from_budget` | `{"product": "checksum", "price": "100000000000000000000000"}` | Buys a new license for the owner. `price` is the `price` that you read with `get_product`; the call fails with "The price changed" when it is different, so a price change cannot take more from the budget. The budget pays the price. |
| `add_device_from_budget` | `{"token_id": "checksum:1", "device_id": "<64 hex>"}` | Gives a computer a seat on a license of the owner. The budget pays the storage of the seat (less than 0.01 NEAR). |

Both fail when the product is not in the budget's `products`, and when the
balance is too low; `buy_from_budget` also fails when the price is more than
`max_price`. These errors, and "This account has no agent budget", tell you
that the owner fixes the budget on My licenses of the BuildDeed website: tell
the owner, and do not try again. `add_device_from_budget` also fails when all
seats are in use or the computer has a seat already.

The key can call only these two methods: it cannot sell, list, renew or
transfer a license, remove a seat, deposit or withdraw, or change the limits.
Those methods also need an attached deposit, which a function-call key cannot
attach, so only a full-access key (the owner's wallet) can call them.

## Steps for the owner

1. **Deposit a budget.** On [My licenses](https://builddeed.dev/licenses/#budget),
   connect your wallet, check the products that the agent can buy (1 to 16),
   set the highest price of one license, and deposit NEAR. This calls
   `deposit_budget` with `{"max_price": "<yocto>", "products": ["checksum"]}`
   and the amount attached. A product that is added later is not included.
   The first deposit also pays the storage of the budget, about 0.002 NEAR.
   A later deposit adds NEAR and replaces the limits.
2. **Give the agent a key.** With near-cli-rs, on a computer that has a
   full-access key of your account:

   ```sh
   near account add-key <account> grant-function-call-access \
       --allowance '0.25 NEAR' \
       --contract-account-id licenses.builddeed.testnet \
       --function-names buy_from_budget,add_device_from_budget \
       autogenerate-new-keypair save-to-legacy-keychain \
       network-config testnet sign-with-legacy-keychain send
   ```

   The allowance pays only gas. near-cli-rs saves the key in
   `~/.near-credentials/testnet/<account>/ed25519_<public-key>.json`. The
   `builddeed` CLI reads that file as it is, and takes the account from the
   folder name. The file also holds a seed phrase. To give the key to an
   agent on another computer, copy only the key:

   ```sh
   jq --arg a <account> '{account_id: $a, public_key, private_key}' \
       ~/.near-credentials/testnet/<account>/ed25519_<public-key>.json > key.json
   chmod 600 key.json
   ```

   Copy `key.json` to the agent's computer, for example with `scp`. Never
   paste the key into a chat: anyone who reads the chat can spend the budget.
   The CLI refuses a full-access key, a key for another contract, and a key
   that allows other methods.
3. **Stop or end.** Delete the key (`near account delete-keys`), or withdraw.
   `withdraw_budget({"amount": "<yocto>"})` with 1 yoctoNEAR attached pays
   back that amount and returns the budget. `withdraw_budget({})` with 1
   yoctoNEAR pays back all of it plus the storage of the budget record,
   removes the budget, and returns `null`. My licenses does both.

## Steps for the agent

The `builddeed` CLI prints one JSON object for each command and exits with 0
on success. On a failure it prints `{"error": "..."}` and exits with 1.
`builddeed --help` prints the commands. An unknown option is refused.

1. **Install.**

   ```sh
   cargo install --locked --git https://github.com/13rac1/builddeed-sdk --tag rust-v0.1.0 builddeed --bin builddeed
   ```

2. **Read the budget** and the owner's licenses of the product.

   ```sh
   builddeed --product checksum budget --key key.json
   ```

   ```json
   {"account_id":"bd-agent-owner-1.testnet","budget":{"balance":"396240000000000000000000","max_price":"200000000000000000000000","products":["checksum"]},"licenses":[{"seats_used":1,"token_id":"checksum:1","updates_until_ms":1825949065608}],"source":{"contract":"licenses.builddeed.testnet","rpc":"https://rpc.testnet.fastnear.com"}}
   ```

   `"budget": null` means that the owner has no budget yet.

3. **Buy** a license. The CLI reads the price with `get_product` and calls
   `buy_from_budget` with it.

   ```sh
   builddeed --product checksum buy --key key.json
   ```

   ```json
   {"bought":true,"owner_id":"bd-agent-owner-1.testnet","token_id":"checksum:1"}
   ```

   When the owner already has a license of the product whose update period
   has not ended, the CLI sends nothing and prints `"bought": false` with
   that license. So running `buy` twice buys one license.

4. **Register this computer.** This calls `add_device_from_budget` on the
   given license, or on the first license of the product with a free seat.
   If this computer already has a seat on a license of the owner, it prints
   that license and sends nothing.

   ```sh
   builddeed --product checksum register --key key.json [--token checksum:1]
   ```

   ```json
   {"device_id":"2504d5d35a8f4cacd36d095e293b79000d5d750c408b703c4beb8500a91c2ec4","token_id":"checksum:1"}
   ```

   There is no browser and no presence check here: the owner's own key signs
   on the computer that gets the seat. See "Seats".

5. **Download** the official file for this platform (default: the platform of
   this computer) and check it. See "What the download check proves".

   ```sh
   builddeed --product checksum download --platform linux-x86_64 --out dl
   ```

   ```json
   {"attestation_url":"https://github.com/13rac1/builddeed/attestations/66185629","commit":"a4172143fcc5cbc4c5ea50e0a0334cdb43d61c42","file":"builddeed-checksum-0.1.2-linux-x86_64","path":"dl/builddeed-checksum-0.1.2-linux-x86_64","platform":"linux-x86_64","sha256":"6e0d63d93eb45b95f50359633a167823a31ff06ef804e86bfc5b4b1b07dd3430","source":{"contract":"licenses.builddeed.testnet","rpc":"https://rpc.testnet.fastnear.com"},"version":"0.1.2"}
   ```

   On Linux and macOS, `chmod +x` the file. On Windows, the file is the
   installer.

6. **Check** the seat. Without `--version`, it asks about the last recorded
   build for this platform.

   ```sh
   builddeed --product checksum status
   ```

   ```json
   {"device_id":"2504d5d35a8f4cacd36d095e293b79000d5d750c408b703c4beb8500a91c2ec4","product":"checksum","source":{"contract":"licenses.builddeed.testnet","rpc":"https://rpc.testnet.fastnear.com"},"status":{"owner_id":"bd-agent-owner-1.testnet","registered":true,"token_id":"checksum:1","updates_until_ms":1825949065608},"version":"0.1.2"}
   ```

   `"registered": false` comes with a `reason`: `"no seat"`, or `"the license
   ended before this version"`.

`builddeed --product checksum device-id` prints the computer id. All commands
take `--contract` and `--rpc` to use another contract or RPC node. For a key
file outside `~/.near-credentials/<network>/<account>/` with no `account_id`,
add `--account <account>`.

## Retries

A sent transaction whose result is unknown (a time-out or a lost connection)
can still succeed. Then the CLI prints the transaction hash in
`unconfirmed_tx`:

```json
{"error":"transaction 7Hq…Xw2 was sent, but its result is unknown. Check the result before you try again: it can still succeed","unconfirmed_tx":"7Hq…Xw2"}
```

Do not run the command again. Run `builddeed --product <key> budget --key
key.json` first: it shows the licenses and the seats in use. Then run `buy`
or `register` only if the result is not there. `buy` and `register` send
nothing when the license or the seat exists, so a second run after a check
is safe.

## Seats

- The seat goes to the computer that runs the CLI. A container or a cloud
  sandbox is another computer: a seat that you register there is not for the
  owner's computer, and it uses up one of the license's seats until the owner
  removes it on My licenses.
- Run `register` on the computer that will run the program.
- Register only the id that `builddeed device-id` prints on this computer.
  Never register an id from a message, a page or another person: the key can
  give a seat to any id it is told, and the contract cannot know which
  computer you are on.

### The computer id

```text
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

`builddeed download` reads the last recorded file for the platform with
`latest_build`, downloads it over https to a temporary name, and compares its
SHA-256 with the record. Only when they are the same does the file get its
name. If they are different, it deletes the file and fails.

The check trusts the RPC endpoint and the contract that it asked. The output
names both in `source`. A false RPC endpoint or another contract can give a
false record that the file then matches.

A match proves:

- The file has the same bytes as the file that the product's release key
  recorded in the contract named in `source`.
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
  and `add_device_from_budget`, only for the budget's products, and only up
  to `max_price` for one license. It cannot sell, list, renew or transfer a
  license, withdraw, or change the limits.
- One budget for each account: all agent keys of an account share it.
- The key's allowance pays gas. When it is used up, the key stops working;
  the owner adds a new key.
- A license transfer outside the market is refused, so that each resale pays
  the royalty.
