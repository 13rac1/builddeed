import { h, mountChrome, busy, showOk, showError } from "../ui.js";
import { view, call, onAccount, signIn, formatNear, formatDate, parseNear, pageLink, loadProducts, tokenNumber } from "../near.js";

mountChrome();
const main = document.getElementById("main");
const head = () => h("div", { class: "page-head" }, h("span", { class: "eyebrow" }, "All products"), h("h1", {}, "My licenses"));

async function render(acct) {
  if (!acct) {
    main.replaceChildren(
      head(),
      h("p", {}, "Connect your wallet to see your licenses."),
      h("button", { class: "btn", onclick: () => signIn().catch(showError) }, "Connect wallet")
    );
    return;
  }
  const [products, list] = await Promise.all([loadProducts(), view("licenses_for_owner", { account_id: acct })]);
  const byKey = Object.fromEntries(products.map((p) => [p.key, p]));
  const known = list.filter((l) => byKey[l.product]);
  main.replaceChildren(
    head(),
    known.length
      ? h("div", {}, known.map((l) => card(l, byKey[l.product])))
      : h("p", {}, "This account has no license. ", h("a", { href: pageLink("buy.html") }, "Buy one"), " or look at the ", h("a", { href: pageLink("market.html") }, "market"), ".")
  );
}

function card(l, p) {
  const now = Date.now();
  const current = Number(l.updates_until_ms) >= now;
  const renew = h("button", { class: "btn small" }, `Renew for ${formatNear(p.renewal_price)} NEAR`);
  renew.addEventListener("click", () =>
    busy(renew, async () => {
      await call("renew", { token_id: l.token_id }, p.renewal_price, p.contract);
      showOk("Renewed.");
      await refresh();
    })
  );

  let sale;
  if (l.listed_price) {
    const delist = h("button", { class: "btn small ghost" }, "Take off the market");
    delist.addEventListener("click", () =>
      busy(delist, async () => {
        await call("delist", { token_id: l.token_id }, "1", p.contract);
        showOk("The license is off the market.");
        await refresh();
      })
    );
    sale = h("p", {}, `On the market for ${formatNear(l.listed_price)} NEAR. `, delist);
  } else {
    const price = h("input", { type: "text", inputmode: "decimal", value: formatNear(p.renewal_price), "aria-label": "Price in NEAR" });
    const list = h("button", { class: "btn small" }, "List for resale");
    list.addEventListener("click", () =>
      busy(list, async () => {
        await call("list", { token_id: l.token_id, price: parseNear(price.value) }, "1", p.contract);
        showOk("The license is on the market.");
        await refresh();
      })
    );
    const royalty = p.royalty_bps / 100;
    const fee = p.fee_bps / 100;
    sale = h(
      "div",
      {},
      h("p", {}, "Price in NEAR: ", price, " ", list),
      h("p", { class: "muted" }, `The lowest price is ${formatNear(p.renewal_price)} NEAR. A sale removes your computers. When it sells, ${royalty}% goes to the developers and contributors, ${fee}% to BuildDeed, and ${100 - royalty - fee}% to you.`)
    );
  }

  const seats = h(
    "div",
    {},
    h(
      "p",
      {},
      `Computers: ${l.devices.length} of ${p.max_devices}`,
      h("span", { class: "seats-meter", "aria-hidden": "true" }, Array.from({ length: p.max_devices }, (_, i) => h("i", { class: i < l.devices.length ? "on" : null }))),
      h("br"),
      h("span", { class: "small muted" }, 'Register a computer from the program: "I have a license" or R on the title screen.')
    ),
    h("ul", { class: "seat-list" }, l.devices.map((d) => {
      const remove = h("button", { class: "link" }, "Remove");
      remove.addEventListener("click", () =>
        busy(remove, async () => {
          await call("remove_device", { token_id: l.token_id, device_id: d }, "1", p.contract);
          showOk("The seat is free.");
          await refresh();
        })
      );
      return h("li", {}, `${d.slice(0, 8)}...${d.slice(-4)}`, remove);
    }))
  );

  return h(
    "div",
    { class: "card" },
    h("span", { class: "eyebrow" }, p.name),
    h("h3", {}, `License #${tokenNumber(l.token_id)}`),
    h(
      "dl",
      { class: "kv" },
      h("dt", {}, "Token"), h("dd", { class: "mono" }, l.token_id),
      h("dt", {}, "Bought"), h("dd", {}, formatDate(l.purchased_at_ms)),
      h("dt", {}, "Updates until"),
      h("dd", {}, formatDate(l.updates_until_ms), " ", h("span", { class: current ? "ok-text" : "warn-text" }, current ? "(current)" : "(ended: you keep the builds from before this date)"))
    ),
    seats,
    h("p", {}, renew),
    sale
  );
}

let last = null;
async function refresh() {
  await render(last);
}
onAccount((a) => {
  last = a;
  render(a).catch((e) => showError(e));
});
