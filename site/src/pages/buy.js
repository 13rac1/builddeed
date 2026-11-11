import { h, mountChrome, busy, showOk } from "../ui.js";
import { view, call, onAccount, currentAccount, formatNear, formatDate, accountLink, FAUCET, REPO, pageLink } from "../near.js";

mountChrome();
const main = document.getElementById("main");
const owned = h("div", {});

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
  const c = await view("get_config");
  const buy = h("button", { class: "btn" }, `Buy for ${formatNear(c.price)} NEAR`);
  buy.addEventListener("click", () =>
    busy(buy, async () => {
      await call("buy", {}, c.price);
      showOk("License bought. Go back to the app and click \"I have a license\".");
      await showOwned();
    })
  );
  main.replaceChildren(
    h("h1", {}, `Buy a license for ${c.product}`),
    h(
      "p",
      { class: "lead" },
      `${formatNear(c.price)} NEAR gives you the official builds released in the next ${c.update_period_days} days, and you keep them. You can resell the license at any time.`
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
      h("li", {}, "An NFT in your NEAR account (NEP-171). It is your license."),
      h("li", {}, "The registered app on up to three computers: no shareware screen, and the folder check is on."),
      h("li", {}, "Each official build released before the end of your update period. Renew to extend it."),
      h("li", {}, "The right to resell. A wallet transfer is refused; use the market, so that the royalty is paid.")
    ),
    h(
      "p",
      { class: "muted" },
      "This is NEAR testnet: the NEAR has no value. Get free testnet NEAR from the ",
      h("a", { href: FAUCET }, "faucet"),
      ". The source code is public: you can also ",
      h("a", { href: REPO }, "build it yourself"),
      "."
    )
  );
}

async function showOwned(acct) {
  acct = acct ?? currentAccount();
  if (!acct) {
    owned.replaceChildren();
    return;
  }
  const list = await view("licenses_for_owner", { account_id: acct });
  owned.replaceChildren(
    list.length
      ? h(
          "div",
          { class: "card" },
          h("h3", {}, "Your licenses"),
          list.map((l) =>
            h("p", {}, `License #${l.token_id}, updates until ${formatDate(l.updates_until_ms)}. `, h("a", { href: pageLink("licenses.html") }, "Manage"))
          )
        )
      : ""
  );
}

render().catch((e) => main.replaceChildren(h("p", { class: "warn-text" }, `Cannot read the contract: ${e.message}`)));
onAccount((a) => showOwned(a).catch(() => {}));
