import { h, mountChrome, busy, showOk, showError, chips } from "../ui.js";
import { view, call, onAccount, currentAccount, formatNear, formatDate, accountLink, productPath, loadProducts, productKey, tokenNumber } from "../near.js";

mountChrome();
const main = document.getElementById("main");

/** The product filter: a product key, or "" for all products. A link with
 * ?product= sets it. */
let filter = productKey(new URLSearchParams(location.search).get("product")) || "";

function setFilter(key) {
  filter = key;
  const url = new URL(location.href);
  if (key) url.searchParams.set("product", key);
  else url.searchParams.delete("product");
  history.replaceState(null, "", url);
  render().catch(showError);
}

async function render() {
  const products = await loadProducts();
  if (filter && !products.some((p) => p.key === filter)) filter = "";
  const byKey = Object.fromEntries(products.map((p) => [p.key, p]));
  const args = filter ? { product: filter, from_index: 0, limit: 100 } : { from_index: 0, limit: 100 };
  const listings = (await view("get_listings", args)).filter((l) => byKey[l.product]);
  const me = currentAccount();

  const filters = chips("Product", [["", "All products"], ...products.map((p) => [p.key, p.name])], filter, setFilter);
  const one = byKey[filter];
  main.replaceChildren(
    h("div", { class: "page-head" }, h("span", { class: "eyebrow" }, one ? one.name : "All products"), h("h1", {}, "Used licenses")),
    h(
      "p",
      { class: "lead" },
      one
        ? `Buy a used ${one.name} license. Of each sale, ${one.royalty_bps / 100}% goes to the developers and contributors and ${one.fee_bps / 100}% to BuildDeed; the seller gets the rest. The license keeps its update period.`
        : "Buy a used license of any BuildDeed product. Of each sale, the product's royalty goes to its developers and contributors and a fee to BuildDeed; the seller gets the rest. The license keeps its update period."
    ),
    filters,
    listings.length
      ? h(
          "div",
          { class: "table-wrap" },
          h(
            "table",
            {},
            h("tr", {}, h("th", {}, "License"), h("th", {}, "Seller"), h("th", {}, "Updates until"), h("th", {}, "Price"), h("th", {}, "")),
            listings.map((l) => row(l, byKey[l.product], me))
          )
        )
      : h("div", { class: "card empty" }, h("p", {}, "No license is on the market now."), h("p", { class: "small" }, "Owners list a license on ", h("a", { href: "/licenses/" }, "My licenses"), ". Or buy a new one ", one ? h("a", { href: productPath(one.key) }, `on the ${one.name} page`) : h("a", { href: "/" }, "in the store"), "."))
  );
}

function row(l, p, me) {
  let action;
  if (me === l.owner_id) {
    action = h("span", { class: "muted" }, "Your listing");
  } else {
    action = h("button", { class: "btn small" }, "Buy");
    action.addEventListener("click", () =>
      busy(action, async () => {
        await call("buy_listed", { token_id: l.token_id, price: l.listed_price }, l.listed_price, p.contract);
        showOk(`${p.name} license #${tokenNumber(l.token_id)} is yours. Register the app with it.`);
        await render();
      })
    );
  }
  const current = Number(l.updates_until_ms) >= Date.now();
  return h(
    "tr",
    {},
    h("td", {}, h("a", { href: productPath(p.key) }, p.name), ` #${tokenNumber(l.token_id)}`),
    h("td", {}, h("a", { href: accountLink(l.owner_id) }, l.owner_id)),
    h("td", {}, formatDate(l.updates_until_ms), current ? "" : h("span", { class: "muted" }, " (ended)")),
    h("td", {}, `${formatNear(l.listed_price)} NEAR`),
    h("td", {}, action)
  );
}

onAccount(() => render().catch(showError));
