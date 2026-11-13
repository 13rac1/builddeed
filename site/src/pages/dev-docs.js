// Docs: the SDK, the release key and build records, and agents.
import { h, mountChrome, busy } from "../ui.js";
import {
  BUILDDEED_REPO,
  SDK_REPO,
  ROOM4DOOM_REPO,
  CONTRACT,
  call,
  onAccount,
  currentAccount,
  parseNear,
  formatNear,
  contractError,
  isProductKey,
} from "../near.js";

mountChrome("developers");
const main = document.getElementById("main");

/** A NEAR amount more than zero as yoctoNEAR, or null. */
function nearAmount(text) {
  try {
    const y = parseNear(text);
    return BigInt(y) > 0n ? y : null;
  } catch {
    return null;
  }
}


const text = (attrs = {}) => h("input", { type: "text", autocomplete: "off", ...attrs });

// ----- Storage for build records -----

const storageKey = text({ id: "s-key", placeholder: "my-app", spellcheck: "false", "aria-label": "Product key" });
const storageAmount = text({ id: "s-amount", inputmode: "decimal", value: "0.5", "aria-label": "NEAR to add" });
const storageStatus = h("div", { "aria-live": "polite" });
const storageButton = h("button", { type: "button", class: "btn small" }, "Add storage");
storageButton.addEventListener("click", () => {
  const key = storageKey.value.trim();
  const amount = nearAmount(storageAmount.value);
  if (!isProductKey(key) || !amount) {
    storageStatus.replaceChildren(h("p", { class: "warn-text" }, "Give a product key and an amount more than zero."));
    return;
  }
  storageStatus.replaceChildren();
  busy(storageButton, async () => {
    try {
      await call("add_storage", { product: key }, amount);
      storageStatus.replaceChildren(h("p", { class: "ok-text" }, `Added ${formatNear(amount)} NEAR to the storage balance of ${key}.`));
    } catch (err) {
      storageStatus.replaceChildren(h("p", { class: "warn-text", role: "alert" }, `The contract refused the call: ${contractError(err)}`));
    }
  });
});

const keyCommand = () => {
  const acct = currentAccount() || "<your-account>";
  return `$ near account add-key ${acct} \\
    grant-function-call-access --allowance '1 NEAR' \\
    --contract-account-id ${CONTRACT} --function-names add_build \\
    autogenerate-new-keypair save-to-legacy-keychain \\
    network-config testnet sign-with-legacy-keychain send
$ jq --arg a ${acct} '{account_id: $a, public_key, private_key}' \\
    ~/.near-credentials/testnet/${acct}/ed25519_<public-key>.json > key.json
$ gh secret set NEAR_RELEASE_KEY --env release < key.json`;
};
const keyPre = h("pre", {}, keyCommand());
onAccount(() => (keyPre.textContent = keyCommand()));

const terminal = (pre) => h("div", { class: "terminal" }, h("div", { class: "tbar" }, h("i"), h("i"), h("i")), pre);

const code = (s) => h("pre", { class: "card mono code" }, s);

main.replaceChildren(
  h(
    "div",
    { class: "page-head" },
    h("h1", {}, "Docs"),
    h("p", { class: "lead" }, "Gate a feature of your program with the BuildDeed SDK, record your releases, and let agents buy for their owners."),
    h(
      "nav",
      { class: "toc", "aria-label": "On this page" },
      h("a", { href: "#sdk" }, "The SDK"),
      h("a", { href: "#check" }, "What your program asks"),
      h("a", { href: "#releases" }, "Releases and build records"),
      h("a", { href: "#agents" }, "Agents")
    )
  ),
  h("h2", { id: "sdk" }, "The SDK"),
  h(
    "p",
    {},
    "The ",
    h("a", { href: SDK_REPO }, "BuildDeed SDK"),
    " (Apache-2.0) checks the license at each start, opens the register page with the presence check, and shows the confirmation code. Its spec and test data let you write the same check in another language."
  ),
  h("h3", {}, "Rust"),
  code(`cargo add builddeed --git ${SDK_REPO} --tag rust-v0.1.2`),
  h("p", { class: "muted small" }, "A Bevy plugin is in the bevy feature. The builddeed command line tool for agents and CI is in the same crate."),
  h("h3", {}, "Go"),
  code("go get github.com/13rac1/builddeed-sdk/go@v0.1.2"),
  h("p", { class: "muted small" }, "The tag in the repository is go/v0.1.2. The module uses the Go standard library only."),
  h(
    "p",
    {},
    "Two programs show the whole flow: ",
    h("a", { href: `${BUILDDEED_REPO}/tree/main/app` }, "BuildDeed Checksum"),
    " (Slint) and ",
    h("a", { href: ROOM4DOOM_REPO }, "Room4Doom"),
    " (a complete Doom engine; R in the game registers). The ",
    h("a", { href: `${SDK_REPO}/blob/main/spec/README.md` }, "protocol spec"),
    " has the exact rules."
  ),
  h("h2", { id: "check" }, "What your program asks"),
  h(
    "p",
    { class: "muted" },
    "One view call to a public NEAR RPC node, with your product key, the computer id (a SHA-256 of the operating system's machine id, the contract name and the product key) and your version:"
  ),
  terminal(
    h(
      "pre",
      {},
      `$ near contract call-function as-read-only ${CONTRACT} \\
    device_license json-args '{"product":"doom","device_id":"6844b8b0…af75","version":"1.0.0"}' \\
    network-config testnet now
{
  "token_id": "doom:1",
  "owner_id": "alice.testnet",
  "updates_until_ms": 1825882492821,
  "covers_version": true
}`
    )
  ),
  h("p", {}, "Keep the program useful without a license, and turn on the extra feature when the answer is not null and covers_version is true."),
  h("h2", { id: "releases" }, "Releases and build records"),
  h(
    "ol",
    {},
    h(
      "li",
      {},
      h("strong", {}, "Give your release workflow a release key. "),
      "Add a function-call access key to your own account. Its receiver is ",
      h("span", { class: "mono" }, CONTRACT),
      " and its only method is add_build. Keep its key file as a secret of the workflow, not a full-access key. With near-cli-rs and the GitHub CLI:"
    )
  ),
  terminal(keyPre),
  h(
    "p",
    { class: "muted small" },
    "The key cannot attach NEAR, so each build record takes its storage from the storage balance of your product. ",
    h("a", { href: `${BUILDDEED_REPO}/blob/main/scripts/record-build.sh` }, "scripts/record-build.sh"),
    " records a release with this key. The release workflow reads a key file with account_id, public_key and private_key; the jq line writes it without the seed phrase of the near-cli-rs file. Agents and the store take the first file that a release records for each platform: record the main download of each platform first."
  ),
  h(
    "ol",
    { start: "2" },
    h(
      "li",
      {},
      h("strong", {}, "Keep the storage balance above zero. "),
      "When add_build says that the storage balance is too low, add NEAR with add_storage. Anyone can pay. The storage balance is in the get_product view."
    )
  ),
  h("div", { class: "card" }, h("p", {}, "Product key ", storageKey, " NEAR ", storageAmount, " ", storageButton), storageStatus),
  h("h2", { id: "agents" }, "Agents"),
  h(
    "p",
    {},
    "AI agents can buy a license, register the computer they run on, and download a checked build with no browser, paid from a budget that the owner sets on My licenses. The ",
    h("a", { href: "/agents.md" }, "guide for agents"),
    " has the views, the calls and the builddeed command line tool. ",
    h("a", { href: "/llms.txt" }, "llms.txt"),
    " points to it."
  )
);
