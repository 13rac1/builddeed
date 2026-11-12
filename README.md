# BuildDeed

Shareware is back, but you can resell your license.

BuildDeed sells software licenses as NFTs on NEAR, to pay open source
developers and contributors. A license gives its owner the verified official
build of a program. Anyone can read the code and build it.

- Website (testnet demo): https://builddeed.dev/ (developers: https://builddeed.dev/developers/)
- License contract: [`licenses.builddeed.testnet`](https://testnet.nearblocks.io/address/licenses.builddeed.testnet): one contract and one NFT collection for every product
- Downloads: [GitHub releases](https://github.com/13rac1/builddeed/releases/latest)
- For AI agents: [agents.md](https://builddeed.dev/agents.md) (buy, register and download with the `builddeed` CLI)
- Built at NEAR HACKATHON 2026, track `nfts` ("NFTs That Do Something").

## The problem

Most open source developers are not paid for their work. Closed licenses pay,
but they hide the code, and nobody can resell a license that they no longer
use. Shareware in the 1990s had a model between the two: try the program
free, then register to remove the nag screen and get the full version.

## What BuildDeed does

- **The license is an NFT that pays people.** Each sale, first or resale,
  splits the payment in the contract among the developer and the
  contributors. A 3% fee goes to BuildDeed.
- **The license gives the official build.** The official app shows a
  shareware "UNREGISTERED" screen, and its registered feature is off, until
  the computer has a seat on a license.
- **A license has three computer seats.** The owner registers a computer with
  one wallet transaction, removes it to free the seat, and a resale clears
  all seats. The app finds its seat in the contract by a computer id that it
  computes from the operating system's machine id, so the id cannot be
  copied to another computer.
- **Self-builds are allowed.** A self-built copy can remove the check; the
  Business Source License permits personal use, and business use needs a
  license.
- **The contract is the record of official builds.** For each release, it
  stores the commit and the SHA-256 of each file, with a link to the GitHub
  build attestation. A recorded file cannot be changed or removed.
- **The Linux and Windows builds are reproducible.** Anyone with Podman can
  rebuild a release from its recorded commit and get the same bytes.
- **Licenses can be resold.** The contract has its own market. A resale pays a
  10% royalty on the listed price to the developer and contributors. A
  wallet transfer (`nft_transfer`) is refused, and a resale price is at least
  the renewal price (0.04 NEAR in the demo), so that a license cannot change
  owner without a royalty. Buyer and seller can still agree a higher price
  and pay the difference outside the contract, and the sale of a whole NEAR
  account moves its licenses with no royalty.
- **A license covers updates for a period.** The demo license covers the
  builds released in the 365 days after purchase. The owner keeps those
  builds after that time, and can renew.

## Try it (judges)

You need a NEAR testnet account with about 10 testnet NEAR. Testnet NEAR is
free: create an account in a wallet that supports testnet (for example
[MyNearWallet testnet](https://testnet.mynearwallet.com/) or Meteor), and get
NEAR from [near-faucet.io](https://near-faucet.io/).

1. Download the app for your system from the
   [latest release](https://github.com/13rac1/builddeed/releases/latest) and
   start it. See "The installers are not signed" below.
   - You see: the "UNREGISTERED COPY" screen. "Continue unregistered" is
     enabled after 5 seconds.
2. Drop any file on the app window, or click "Choose a file…".
   - You see: its SHA-256. Drop the installer that you downloaded: the app
     shows "Official build <version>", the commit and the attestation link.
3. Drop a folder on the window, or click "Check a folder…".
   - You see: "The folder check needs a license."
4. Click "Register", then "Buy a license". On the website, connect your
   wallet and click "Buy for 0.1 NEAR".
   - You see: one wallet approval, then "License bought". On "My licenses":
     license #N, updates until one year from now.
5. In the app, click "I have a license". The browser opens the register page
   with the computer id. Click "Register this computer" on your license.
   - You see: one wallet approval (about 0.01 NEAR for storage). Within a few
     seconds the app shows "Registered to <your account>, updates until
     <date>" and the nag screen is gone. "My licenses" shows 1 of 3 seats.
6. Drop the same folder twice, with a change between the two.
   - You see: a report of the changed, added and removed files.
7. On the website, go to "My licenses" and list the license for 0.2 NEAR. With
   a second testnet account, buy it on "Market".
   - You see: the second account owns the license. The first account got
     0.174 NEAR; the developer, the contributor and the fee account got the
     royalty and the fee. The resale cleared the seats: restart the app of
     the first account, and it shows the
     nag screen again.
8. Go to "Verify a download" and drop a downloaded file.
   - You see: "Official build <version> for <platform>", or "no record" for any
     other file.

### Rebuild a release and compare (optional)

With Podman (about 6 GB of disk for the build cache):

```sh
git clone https://github.com/13rac1/builddeed && cd builddeed
scripts/verify.sh 0.1.1
```

You see `SAME` for each Linux and Windows file, compared with the SHA-256 in
the contract. The script checks that the tag `v0.1.1` points to the commit
that the contract records, and builds with the scripts of your checkout.

### The installers are not signed

The hackathon release has no Apple or Windows code signature.

- macOS: open the `.dmg`, drag the app to Applications, then right-click the
  app and choose "Open". On macOS 15, if it is blocked, go to System
  Settings, Privacy & Security, and click "Open Anyway".
- Windows: SmartScreen shows "Windows protected your PC". Click "More info",
  then "Run anyway". The installer installs for the current user only.
- Linux: `sudo apt install ./builddeed-checksum_<version>_amd64.deb`, or extract
  the `.tar.gz` and run `builddeed-checksum`. The file and folder dialogs use
  the XDG desktop portal (`xdg-desktop-portal`). On Wayland, drag and drop is
  not available: use the "Choose a file…" and "Check a folder…" buttons.

## How NEAR is used

| Part | NEAR feature |
| --- | --- |
| License | NEP-171 NFT with NEP-177 metadata and NEP-181 enumeration. The token metadata holds the update period, and each license shows its product's image (`<product>.svg` under the collection's `base_uri`, served by the website). |
| Payment split | The contract pays the payees with transfers in the `buy`, `renew` and `buy_listed` calls. |
| Resale market and royalty | `list`, `delist` (1 yoctoNEAR, so that only a full-access key can call them), `buy_listed` with the price that the buyer saw. `nft_transfer` is refused. |
| Products | One contract holds every product. A developer calls `add_product` with the key, name, prices, update period, royalty, payees and seats per license, and pays only for the storage of the record. Token ids are `<product>:<number>`, for example `doom:1`. |
| Computer seats | `add_device` (owner only, storage paid by the owner), `remove_device`, `device_license(product, device_id, version)`. Three seats for each license; a resale clears them. The app reads its seat with a view call, so registration needs no license server and no callback to the app. |
| Build record | `add_build` (append-only, per product), `find_artifact`, `get_builds`. The storage of a record comes from the product's storage balance (`add_storage`), because a function-call key cannot attach NEAR. |
| Agent budget | `deposit_budget` (full-access key) with a highest price and a list of products, then a function-call key of the owner can call `buy_from_budget` (with the price it read) and `add_device_from_budget`: an agent buys and registers with no browser and cannot sell. `withdraw_budget` returns the rest. |
| Download links | Each recorded file has its download URL; `latest_build(product, platform)` gives the newest one. |
| Release key | A function-call access key of the developer account that can call only `add_build` on the license contract. The release workflow holds it, not a full-access key. |

## Repository

| Path | What it is |
| --- | --- |
| `contract/` | The license contract (Rust, near-sdk 5.30). Unit tests in `src/tests.rs`, a near-sandbox test in `tests/sandbox.rs`. |
| `app/` | BuildDeed Checksum, the demo app (Rust, Slint 1.18). The window is in `app/ui/app.slint`. |
| `site/` | The website (Vite, NEAR Connect). |
| `build/` | The Podman build image (Rust 1.99.1, pinned by digest; Debian packages from a fixed snapshot) and the NSIS installer script. |
| `scripts/` | Build, package, record and verify scripts. All Rust builds run in Podman. |
| `.github/workflows/` | CI (all checks), Release (check, build, attest, record on chain, publish) and Pages. |

### Build and test

You need Podman.

```sh
scripts/check.sh             # everything below, with rustfmt and clippy
scripts/contract.sh test     # contract: 122 unit tests and the sandbox test
scripts/app.sh test          # app: 4 tests (seat check against a local fake RPC)
scripts/app.sh linux         # app/target/x86_64-unknown-linux-gnu/release/builddeed-checksum
scripts/app.sh windows       # app/target/x86_64-pc-windows-gnu/release/builddeed-checksum.exe
scripts/package.sh           # dist/: .deb, .tar.gz, NSIS installer, SHA256SUMS
```

The website: `cd site && npm ci && npm run build` (Node 24).

### Deployed contract

The contract on `licenses.builddeed.testnet` has the code hash
`ee9dcf6665596a4dc63a6c6ca131fa52d89790b4020390bdb8e28728c5d89869`.
`scripts/contract.sh build` prints the same SHA-256.

## Trust model and limits

- BuildDeed holds a full-access key of the contract account and can deploy
  new code. A future version puts upgrades behind a time lock, so that
  owners see a change before it starts.
- Developers cannot change the code or the fee. The BuildDeed fee is set
  when the contract is created; no method changes it.
- Products are isolated: a developer can record builds and change terms only
  for their own product. Build records, file hashes and seats are kept per
  product.
- The developer can change the payees and the prices of future sales at any
  time. The renewal price is also the lowest resale price, so a developer who
  raises it can price resales out of the market. The royalty and the seats
  per license are fixed when the product is added.
- A stolen release key can record a false build, or take a future version
  name. It cannot change or remove a recorded build, and it can add files to
  a version only in the 24 hours after the first record of that version.
  `verify.sh` refuses a version whose tag does not point to the recorded
  commit.
- The computer id comes from the machine id of the operating system. A user
  who controls the computer can change the app to report another id; that is
  the same as building the app without the check.
- Computers made from one cloned disk image (virtual machines, containers)
  can share one machine id, and so one seat.
- There is no offline mode. The app asks the contract for its seat at each
  start; with no network it runs unregistered. A result stored on the
  computer could be copied or edited, so the app keeps none.
- The registration check is in public source code. It works because most users
  and every business prefer to pay than to rebuild, as with shareware.
- macOS builds are not reproducible. They have a provenance attestation only.
- This demo runs on NEAR testnet. Testnet NEAR has no value.

## Hackathon disclosures

- Hackathon window: the project started on 2026-11-10. There is no code from
  before the hackathon.
- AI-written code: an AI coding agent (Claude Code) wrote most of the code,
  the tests and the documents with the team, who directed the design and
  reviewed the work.
- Libraries: near-sdk, near-contract-standards, near-workspaces (contract);
  Slint, rfd, ed25519-dalek, sha2, ureq, serde, base64, bs58, getrandom,
  hex (app); @hot-labs/near-connect, Vite (site). The full lists are in
  `contract/Cargo.lock`, `app/Cargo.lock` and `site/package-lock.json`.
- No template was used.

## License

Business Source License 1.1. See [LICENSE](LICENSE). This is a
source-available license, not an open source license: the code is public,
and each version becomes open source (Apache License 2.0) four years after
its release. Personal use is free. Business use needs a BuildDeed license.

The user interface of the app uses [Slint](https://slint.dev) under the
Slint Royalty-free Desktop, Mobile, and Web Applications License 2.0. The
app shows the Slint attribution in Help, "About BuildDeed Checksum".
