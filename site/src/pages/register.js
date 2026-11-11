// The app opens this page as
// register.html#product=<key>&device=<64 hex>&version=<x.y.z>
// (no product: the demo).
// The owner picks a license and registers the computer with one wallet
// transaction (add_device). The app sees the seat a few seconds later.
import { h, mountChrome, busy, showError } from "../ui.js";
import { view, call, onAccount, signIn, formatDate, pageLink } from "../near.js";

mountChrome();
const main = document.getElementById("main");

const params = new URLSearchParams(location.hash.slice(1));
const device = params.get("device") || "";
const version = params.get("version") || "";
// Enough for the storage of one seat; the contract returns the excess.
const SEAT_DEPOSIT = "10000000000000000000000"; // 0.01 NEAR

const short = (d) => `${d.slice(0, 8)}...${d.slice(-4)}`;

function invalid() {
  main.replaceChildren(
    h("div", { class: "page-head" }, h("h1", {}, "Register a computer")),
    h("p", { class: "warn-text" }, "This link has no valid computer id."),
    h("p", {}, 'Open this page from the app: click "I have a license" in BuildDeed Checksum.')
  );
}

async function render(acct) {
  const intro = [
    h("h1", {}, `Register this computer for ${config.product}`),
    h(
      "p",
      { class: "lead" },
      `Computer ${short(device)} asks for a seat on one of your licenses. A license has ${
        config.max_devices
      } seats. The app checks the seat every few seconds and unlocks when it finds it.`
    ),
  ];
  if (!acct) {
    main.replaceChildren(
      ...intro,
      h("button", { class: "btn", onclick: () => signIn().catch(showError) }, "Connect wallet")
    );
    return;
  }
  const [licenses, seat] = await Promise.all([
    view("licenses_for_owner", { account_id: acct }),
    view("device_license", { device_id: device, version: version || "0" }),
  ]);
  if (seat) {
    main.replaceChildren(
      ...intro,
      h(
        "div",
        { class: "card" },
        h("p", { class: "ok-text" }, `This computer is registered on license #${seat.token_id} of ${seat.owner_id}.`),
        h("p", {}, "Go back to the app. It unlocks within a few seconds.")
      )
    );
    return;
  }
  if (!licenses.length) {
    main.replaceChildren(
      ...intro,
      h("p", {}, "This account has no license. ", h("a", { href: pageLink("buy.html") }, "Buy one"), ", then come back to this page.")
    );
    return;
  }
  main.replaceChildren(...intro, ...licenses.map(card));
}

function card(l) {
  const full = l.devices.length >= config.max_devices;
  const add = h("button", { class: "btn", disabled: full }, "Register this computer");
  add.addEventListener("click", () =>
    busy(add, async () => {
      await call("add_device", { token_id: l.token_id, device_id: device }, SEAT_DEPOSIT);
      await render(currentAcct);
    })
  );
  return h(
    "div",
    { class: "card" },
    h("h3", {}, `License #${l.token_id}`),
    h("p", {}, `Updates until ${formatDate(l.updates_until_ms)}. Seats: ${l.devices.length} of ${config.max_devices}.`),
    full
      ? h("p", { class: "muted" }, "All seats are in use. Remove a computer on ", h("a", { href: pageLink("licenses.html") }, "My licenses"), " first.")
      : add,
    h("p", { class: "muted" }, "One wallet transaction. It costs about 0.01 NEAR for storage; the contract returns the rest.")
  );
}

let config;
let currentAcct = null;
if (!/^[0-9a-f]{64}$/.test(device)) {
  invalid();
} else {
  view("get_config")
    .then((c) => {
      config = c;
      onAccount((a) => {
        currentAcct = a;
        render(a).catch(showError);
      });
    })
    .catch(showError);
}
