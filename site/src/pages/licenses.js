import { h, mountChrome, busy, showOk, showError } from "../ui.js";
import { view, call, onAccount, signIn, formatNear, formatDate, parseNear } from "../near.js";

mountChrome();
const main = document.getElementById("main");
let config = null;

async function render(acct) {
  if (!acct) {
    main.replaceChildren(
      h("h1", {}, "My licenses"),
      h("p", {}, "Connect your wallet to see your licenses."),
      h("button", { class: "btn", onclick: () => signIn().catch(showError) }, "Connect wallet")
    );
    return;
  }
  config = config ?? (await view("get_config"));
  const list = await view("licenses_for_owner", { account_id: acct });
  main.replaceChildren(
    h("h1", {}, "My licenses"),
    list.length
      ? h("div", {}, list.map(card))
      : h("p", {}, "This account has no license. ", h("a", { href: "buy.html" }, "Buy one"), " or look at the ", h("a", { href: "market.html" }, "market"), ".")
  );
}

function card(l) {
  const now = Date.now();
  const current = Number(l.updates_until_ms) >= now;
  const renew = h("button", { class: "btn small" }, `Renew for ${formatNear(config.renewal_price)} NEAR`);
  renew.addEventListener("click", () =>
    busy(renew, async () => {
      await call("renew", { token_id: l.token_id }, config.renewal_price);
      showOk("Renewed.");
      await refresh();
    })
  );

  let sale;
  if (l.listed_price) {
    const delist = h("button", { class: "btn small ghost" }, "Take off the market");
    delist.addEventListener("click", () =>
      busy(delist, async () => {
        await call("delist", { token_id: l.token_id }, "1");
        showOk("The license is off the market.");
        await refresh();
      })
    );
    sale = h("p", {}, `On the market for ${formatNear(l.listed_price)} NEAR. `, delist);
  } else {
    const price = h("input", { type: "text", inputmode: "decimal", value: "3", "aria-label": "Price in NEAR" });
    const list = h("button", { class: "btn small" }, "List for resale");
    list.addEventListener("click", () =>
      busy(list, async () => {
        await call("list", { token_id: l.token_id, price: parseNear(price.value) }, "1");
        showOk("The license is on the market.");
        await refresh();
      })
    );
    const royalty = config.royalty_bps / 100;
    const fee = config.fee_bps / 100;
    sale = h(
      "div",
      {},
      h("p", {}, "Price in NEAR: ", price, " ", list),
      h("p", { class: "muted" }, `The lowest price is ${formatNear(config.renewal_price)} NEAR. A sale removes your computers. When it sells, ${royalty}% goes to the developers and contributors, ${fee}% to BuildDeed, and ${100 - royalty - fee}% to you.`)
    );
  }

  const seats = h(
    "div",
    {},
    h("p", {}, `Computers: ${l.devices.length} of ${config.max_devices}. Register a computer from the app ("I have a license").`),
    l.devices.map((d) => {
      const remove = h("button", { class: "link" }, "Remove");
      remove.addEventListener("click", () =>
        busy(remove, async () => {
          await call("remove_device", { token_id: l.token_id, device_id: d }, "1");
          showOk("The seat is free.");
          await refresh();
        })
      );
      return h("p", { class: "mono" }, `${d.slice(0, 8)}...${d.slice(-4)} `, remove);
    })
  );

  return h(
    "div",
    { class: "card" },
    h("h3", {}, `License #${l.token_id}`),
    h(
      "dl",
      { class: "kv" },
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
