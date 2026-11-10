import { h, mountChrome, busy, showOk, showError } from "../ui.js";
import { view, call, onAccount, currentAccount, formatNear, formatDate, accountLink } from "../near.js";

mountChrome();
const main = document.getElementById("main");

async function render() {
  const [c, listings] = await Promise.all([view("get_config"), view("get_listings", { from_index: 0, limit: 100 })]);
  const me = currentAccount();
  main.replaceChildren(
    h("h1", {}, "License market"),
    h(
      "p",
      { class: "lead" },
      `Buy a used ${c.product} license. Of each sale, ${c.royalty_bps / 100}% goes to the developers and contributors and ${c.fee_bps / 100}% to BuildDeed; the seller gets the rest. The license keeps its update period.`
    ),
    listings.length
      ? h(
          "div",
          { class: "table-wrap" },
          h(
            "table",
            {},
            h("tr", {}, h("th", {}, "License"), h("th", {}, "Seller"), h("th", {}, "Updates until"), h("th", {}, "Price"), h("th", {}, "")),
            listings.map((l) => row(l, me))
          )
        )
      : h("p", {}, "No license is on the market now. Owners can list a license on ", h("a", { href: "licenses.html" }, "My licenses"), ".")
  );
}

function row(l, me) {
  let action;
  if (me === l.owner_id) {
    action = h("span", { class: "muted" }, "Your listing");
  } else {
    action = h("button", { class: "btn small" }, "Buy");
    action.addEventListener("click", () =>
      busy(action, async () => {
        await call("buy_listed", { token_id: l.token_id, price: l.listed_price }, l.listed_price);
        showOk(`License #${l.token_id} is yours. Register the app with it.`);
        await render();
      })
    );
  }
  const current = Number(l.updates_until_ms) >= Date.now();
  return h(
    "tr",
    {},
    h("td", {}, `#${l.token_id}`),
    h("td", {}, h("a", { href: accountLink(l.owner_id) }, l.owner_id)),
    h("td", {}, formatDate(l.updates_until_ms), current ? "" : h("span", { class: "muted" }, " (ended)")),
    h("td", {}, `${formatNear(l.listed_price)} NEAR`),
    h("td", {}, action)
  );
}

onAccount(() => render().catch(showError));
