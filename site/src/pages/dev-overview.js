// The developer overview: why sell here, the cut, one collection, trust.
import { h, mountChrome } from "../ui.js";
import { BUILDDEED_REPO, CONTRACT, accountLink } from "../near.js";

mountChrome("developers");
const main = document.getElementById("main");

const reasons = [
  [
    "Keep the code public",
    "BuildDeed brings back the shareware deal for programs whose source code is public. The official build turns on its extra feature when the computer has a seat on a license. People who build it themselves are not your customers; businesses and most users pay for the official build.",
  ],
  [
    "Set your own terms",
    "You choose the price, the renewal price (also the lowest resale price), the update period, the royalty, the number of computer seats, and up to 16 payees: you and your contributors, each with a share.",
  ],
  [
    "Ship verified builds",
    "Build in a pinned container so that anyone can rebuild the same bytes. Your release workflow attests each file on GitHub and records its SHA-256 on chain. Buyers check a download against that record.",
  ],
  [
    "Get paid, again and again",
    "Each first sale and renewal is split among your payees in the same transaction. Each resale pays your royalty. BuildDeed takes 3%.",
  ],
];

const stores = [
  ["BuildDeed", "3%", "3% fee + your royalty", "Yes, the source stays public"],
  ["Apple App Store", "15-30%", "Not possible", "No"],
  ["Google Play", "15-30%", "Not possible", "No"],
  ["Microsoft Store", "12-15%", "Not possible", "No"],
];

const collection = [
  ["No contract to deploy", "You add your product with one call. There is no contract of yours to write, audit, deploy or keep up."],
  ["A fee that nobody can remove", "The 3% fee and the payments are in the contract code. A buyer cannot buy around them, and a resale outside the market is refused, so your royalty is paid."],
  ["One contract to audit", "Every product uses the same reviewed code. Buyers and wallets trust one contract, not one for each developer."],
  ["One collection in wallets and markets", "All licenses are tokens of one NFT collection, \"BuildDeed licenses\". A wallet or a market that shows it shows your product too."],
];

main.replaceChildren(
  h(
    "div",
    { class: "page-head" },
    h("h1", {}, "Sell your software. Keep it open."),
    h(
      "p",
      { class: "lead" },
      "Put your program in the BuildDeed store. Each license is an NFT that pays you and your contributors when it is sold, and again each time it is resold."
    ),
    h("div", { class: "btn-row" }, h("a", { class: "btn", href: "/developers/add/" }, "Add your product"), h("a", { class: "btn ghost", href: "/developers/docs/" }, "Read the docs"))
  ),
  h(
    "div",
    { class: "steps two" },
    reasons.map(([title, body], i) =>
      h("div", { class: "card step" }, h("span", { class: "num" }, `0${i + 1}`), h("h3", {}, title), h("p", { class: "muted" }, body))
    )
  ),
  h("h2", { style: "margin-top:40px" }, "The cut, compared"),
  h("p", { class: "muted" }, "Listed commission rates for paid software. The lower store rates apply to small businesses or to some categories."),
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
  h("h2", { style: "margin-top:40px" }, "One collection: what it means for you"),
  h(
    "p",
    { class: "muted" },
    "Every license of every product is a token of one contract, ",
    h("a", { class: "mono", href: accountLink(CONTRACT) }, CONTRACT),
    "."
  ),
  h("div", { class: "grid" }, collection.map(([title, body]) => h("div", { class: "card" }, h("h3", {}, title), h("p", { class: "muted" }, body)))),
  h("h2", { style: "margin-top:40px" }, "The trust model"),
  h(
    "ul",
    { class: "trust" },
    h("li", {}, h("strong", {}, "BuildDeed "), "holds the full-access key of the contract and can deploy new code. You cannot change the code or the fee."),
    h("li", {}, h("strong", {}, "You "), "are the developer of your product: you can change the payees and the prices of future sales. The royalty and the seats per license are fixed when you add the product. The renewal price is also the lowest resale price."),
    h("li", {}, h("strong", {}, "Products are kept apart: "), "only you can record builds and change terms for your product, and only for it."),
    h("li", {}, h("strong", {}, "Your release key "), "can only record builds. A stolen release key could record a false build of a new version. It cannot change or remove a recorded one, and verify.sh refuses a version whose tag does not match the record."),
    h("li", {}, h("strong", {}, "The license check "), "is in your public code. A user can build the program without it. A user with administrator rights can copy a machine id, so the seat limit holds against copied program files, not against a copied machine id.")
  ),
  h(
    "div",
    { class: "cta-band", style: "margin-top:40px" },
    h(
      "div",
      {},
      h("h2", {}, "Ready to sell?"),
      h("p", {}, "This is a testnet demo built at NEAR HACKATHON 2026. Add a product on testnet, or open an issue to talk.")
    ),
    h(
      "div",
      { class: "btn-row", style: "margin:0" },
      h("a", { class: "btn", href: "/developers/add/" }, "Add your product"),
      h("a", { class: "btn ghost", href: `${BUILDDEED_REPO}/issues` }, "Open an issue")
    )
  )
);
