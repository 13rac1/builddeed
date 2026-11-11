import { h, mountChrome } from "../ui.js";
import { BUILDDEED_REPO, PRODUCTS } from "../near.js";

mountChrome();
const main = document.getElementById("main");

const steps = [
  [
    "Set your terms",
    "Each product has its own license contract on a NEAR account. You choose the price, the renewal price (also the lowest resale price), the update period, the royalty, the number of computer seats, and up to 16 payees: you and your contributors, each with a share.",
  ],
  [
    "Gate a feature, not the program",
    "Your official build asks the contract one question at start: does this computer have a seat on a license that covers this version? Keep the program useful without a license; turn on the extra feature when the answer is yes.",
  ],
  [
    "Ship verified builds",
    "Build in a pinned container so that anyone can rebuild the same bytes. The release workflow attests each file on GitHub and records its SHA-256 in the contract with a key that can call only add_build. A recorded file cannot be changed.",
  ],
  [
    "Get paid, again and again",
    "Every first sale and renewal is split among your payees in the same transaction. Every resale pays your royalty. BuildDeed takes 3%.",
  ],
];

const stores = [
  ["BuildDeed", "3%", "3% fee + your royalty (10% in the demo)", "Yes, the source stays public"],
  ["Apple App Store", "15-30%", "Not possible", "No"],
  ["Google Play", "15-30%", "Not possible", "No"],
  ["Microsoft Store", "12-15%", "Not possible", "No"],
];

main.replaceChildren(
  h(
    "div",
    { class: "page-head" },
    h("span", { class: "eyebrow" }, "For developers"),
    h("h1", {}, "Sell your software. Keep it open."),
    h(
      "p",
      { class: "lead" },
      "BuildDeed brings back the shareware deal for programs whose source code is public: the official build nags until the user buys a license, and the license is an NFT that pays you and your contributors every time it is sold."
    )
  ),
  h(
    "div",
    { class: "steps two" },
    steps.map(([title, text], i) =>
      h("div", { class: "card step" }, h("span", { class: "num" }, `0${i + 1}`), h("h3", {}, title), h("p", { class: "muted" }, text))
    )
  ),
  h("h2", { style: "margin-top:40px" }, "The cut, compared"),
  h(
    "p",
    { class: "muted" },
    "Listed commission rates for paid software. The lower store rates apply to small businesses or to some categories."
  ),
  h(
    "div",
    { class: "card table-wrap" },
    h(
      "table",
      {},
      h("tr", {}, h("th", {}, "Where"), h("th", {}, "Cut of a sale"), h("th", {}, "Resale of a license"), h("th", {}, "Source can be public")),
      stores.map((row, i) => h("tr", { class: i === 0 ? "highlight" : null }, row.map((c) => h("td", {}, c))))
    )
  ),
  h("h2", { style: "margin-top:40px" }, "What your program asks"),
  h(
    "p",
    { class: "muted" },
    "One view call to a public NEAR RPC node, with the computer id (a SHA-256 of the operating system's machine id and your contract name) and your version:"
  ),
  h(
    "div",
    { class: "terminal" },
    h("div", { class: "tbar" }, h("i"), h("i"), h("i")),
    h(
      "pre",
      {},
      `$ near contract call-function as-read-only ${PRODUCTS.doom.contract} \\
    device_license json-args '{"device_id":"6844b8b0…af75","version":"0.2.0"}' \\
    network-config testnet now
{
  "token_id": "1",
  "owner_id": "alice.testnet",
  "updates_until_ms": 1825882492821,
  "covers_version": true
}`
    )
  ),
  h(
    "p",
    {},
    "Two working integrations show the whole flow: ",
    h("a", { href: `${BUILDDEED_REPO}/tree/main/app` }, "BuildDeed Checksum"),
    " (egui) and ",
    h("a", { href: PRODUCTS.doom.repo }, "Iron Doom"),
    " (Bevy, registration from the title screen). A Rust SDK that packages this code is next."
  ),
  h("h2", { style: "margin-top:40px" }, "What buyers have to trust"),
  h(
    "ul",
    {},
    h("li", {}, "You hold the developer key of your contract: you can change payees and prices of future sales, and deploy new code."),
    h("li", {}, "A stolen release key could record a false build. It cannot change or remove a recorded one, and verify.sh refuses a version whose tag does not match the record."),
    h("li", {}, "The license check is in your public code. People who rebuild without it are not your customers; businesses and most users pay for the official build.")
  ),
  h(
    "div",
    { class: "cta-band", style: "margin-top:40px" },
    h(
      "div",
      {},
      h("h2", {}, "Want your program on BuildDeed?"),
      h("p", {}, "This is a testnet demo built at NEAR HACKATHON 2026. Read the code, try the two products, and open an issue to talk.")
    ),
    h(
      "div",
      { class: "btn-row", style: "margin:0" },
      h("a", { class: "btn", href: `${BUILDDEED_REPO}/issues` }, "Open an issue"),
      h("a", { class: "btn ghost", href: BUILDDEED_REPO }, "Read the code")
    )
  )
);
