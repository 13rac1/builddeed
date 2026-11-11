import { h, mountChrome, busy, showOk } from "../ui.js";
import { viewOf, call, onAccount, currentAccount, formatNear, formatDate, tokenNumber, accountLink, FAUCET, pageLink, loadProduct } from "../near.js";

mountChrome();
const main = document.getElementById("main");
const owned = h("div", {});
/** The product of this page, after it loads. */
let product = null;

function splitRows(c) {
  // The same arithmetic as the contract, without the token storage cost.
  const price = BigInt(c.price);
  const fee = (price * BigInt(c.fee_bps)) / 10000n;
  const rest = price - fee;
  return [
    ["BuildDeed fee", c.fee_account, fee],
    ...c.payees.map((p) => [`${p.bps / 100}% of the rest`, p.account_id, (rest * BigInt(p.bps)) / 10000n]),
  ];
}

async function render() {
  const c = (product = await loadProduct());
  const buy = h("button", { class: "btn" }, `Buy for ${formatNear(c.price)} NEAR`);
  buy.addEventListener("click", () =>
    busy(buy, async () => {
      await call("buy", { product: c.key }, c.price, c.contract);
      showOk("License bought. Go back to the app and click \"I have a license\".");
      await showOwned();
    })
  );
  main.replaceChildren(
    h("div", { class: "page-head" }, h("span", { class: "eyebrow" }, c.name), h("h1", {}, "Buy a license")),
    h(
      "p",
      { class: "lead" },
      `${formatNear(c.price)} NEAR gives you the official builds of ${c.name} released in the next ${c.update_period_days} days, and you keep them. Use it on ${c.max_devices} computers, and resell it at any time.`
    ),
    h(
      "div",
      { class: "card" },
      h("h3", {}, "Where the money goes"),
      h(
        "div",
        { class: "table-wrap" },
        h(
          "table",
          {},
          h("tr", {}, h("th", {}, "Share"), h("th", {}, "Account"), h("th", {}, "NEAR")),
          splitRows(c).map(([label, acct, amount]) =>
            h(
              "tr",
              {},
              h("td", {}, label),
              h("td", {}, h("a", { href: accountLink(acct) }, acct)),
              h("td", {}, formatNear(amount.toString()))
            )
          )
        )
      ),
      h(
        "p",
        { class: "muted" },
        "The storage of your license token (about 0.01 NEAR) comes out of the price before the split. The contract sends the payments in the same transaction."
      ),
      buy
    ),
    owned,
    h("h2", {}, "What you get"),
    h(
      "ul",
      {},
      h("li", {}, "An NFT in your NEAR account (NEP-171) in the BuildDeed licenses collection. It is your license."),
      h("li", {}, `The registered program on up to ${c.max_devices} computers: no shareware screen, and the registered features are on.`),
      h("li", {}, "Each official build released before the end of your update period. Renew to extend it."),
      h("li", {}, "The right to resell. A wallet transfer is refused; use the market, so that the royalty is paid.")
    ),
    h(
      "p",
      { class: "muted" },
      "This is NEAR testnet: the NEAR has no value. Get free testnet NEAR from the ",
      h("a", { href: FAUCET }, "faucet"),
      ".",
      c.repo ? [" The source code is public: you can also ", h("a", { href: c.repo }, "build it yourself"), "."] : ""
    )
  );
}

async function showOwned(acct) {
  acct = acct ?? currentAccount();
  if (!acct || !product) {
    owned.replaceChildren();
    return;
  }
  const list = await viewOf(product, "licenses_for_owner", { account_id: acct, product: product.key });
  owned.replaceChildren(
    list.length
      ? h(
          "div",
          { class: "card" },
          h("h3", {}, "Your licenses"),
          list.map((l) =>
            h("p", {}, `License #${tokenNumber(l.token_id)}, updates until ${formatDate(l.updates_until_ms)}. `, h("a", { href: pageLink("licenses.html") }, "Manage"))
          )
        )
      : ""
  );
}

render()
  .then(() => onAccount((a) => showOwned(a).catch(() => {})))
  .catch((e) => main.replaceChildren(h("p", { class: "warn-text" }, `Cannot read the contract: ${e.message}`)));
