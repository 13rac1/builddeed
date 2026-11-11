// The program opens this page as
// register.html#product=<key>&device=<64 hex>&version=<x.y.z>&port=<p>&code=<32 hex>
// (no product: BuildDeed Checksum; the old key "demo" also means Checksum).
//
// Presence check: before it asks the owner's wallet, the page asks the
// program on this computer (http://127.0.0.1:<port>/builddeed/<code>). The
// program answers with its computer id. A link that someone sent from another
// computer has no program behind it here, so the page refuses it. When the
// browser blocks the request, the page asks for the 6-digit confirmation code
// that the program shows. The code comes from the computer id, so a link for
// another computer expects another code. See spec/README.md in the BuildDeed
// SDK.
//
// The owner picks a license and registers the computer with one wallet
// transaction (add_device). The program sees the seat a few seconds later.
import { h, mountChrome, busy, showError } from "../ui.js";
import { viewOf, call, onAccount, signIn, formatDate, pageLink, loadProduct, tokenNumber, PRODUCT_KEY } from "../near.js";

mountChrome();
const main = document.getElementById("main");

const params = new URLSearchParams(location.hash.slice(1));
const device = params.get("device") || "";
const version = params.get("version") || "";
const port = Number(params.get("port"));
const code = params.get("code") || "";
// Enough for the storage of one seat; the contract returns the excess.
const SEAT_DEPOSIT = "10000000000000000000000"; // 0.01 NEAR
// Some browsers ask the user before a page may reach 127.0.0.1: give them time.
const PRESENCE_WAIT_MS = 20000;

const short = (d) => `${d.slice(0, 8)}...${d.slice(-4)}`;

/** "checking", "confirmed", "other" (another computer), "blocked" or "none". */
let presence = "checking";
/** True when the owner typed the confirmation code of this link's computer. */
let codeOk = false;
/** The code that the program for `device` shows; set after the hash. */
let expectedCode = null;

/** decimal(first 4 bytes of SHA-256("builddeed-confirm-v1\n" + id)) mod 1e6, 6 digits. */
async function confirmCode(id) {
  const hash = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(`builddeed-confirm-v1\n${id}`));
  const n = new DataView(hash).getUint32(0, false);
  return String(n % 1000000).padStart(6, "0");
}

async function checkPresence() {
  if (!Number.isInteger(port) || port < 1024 || port > 65535 || !/^[0-9a-f]{32}$/.test(code)) {
    return "none";
  }
  try {
    const res = await fetch(`http://127.0.0.1:${port}/builddeed/${code}`, {
      cache: "no-store",
      signal: AbortSignal.timeout(PRESENCE_WAIT_MS),
    });
    if (!res.ok) return "other";
    const answer = await res.json();
    return answer.device_id === device && answer.contract === PRODUCT.contract ? "confirmed" : "other";
  } catch {
    return "blocked";
  }
}

function presenceNote() {
  switch (presence) {
    case "checking":
      return h(
        "div",
        { class: "card" },
        h("div", { class: "skeleton", style: "width:40%" }),
        h("p", { class: "muted small" }, `Checking that ${PRODUCT.name} on this computer opened this page. If your browser asks to let this site reach apps on this device, allow it.`)
      );
    case "confirmed":
      return h("p", { class: "ok-text" }, `${PRODUCT.name} on this computer opened this page: this is the computer that gets the seat.`);
    case "other":
      return h(
        "div",
        { class: "card" },
        h("p", { class: "warn-text" }, "This link is for another computer. Do not register it."),
        h("p", { class: "muted" }, `To register this computer, open ${PRODUCT.name} here and use "I have a license" (R on the Iron Doom title screen).`)
      );
    default: {
      const input = h("input", {
        id: "confirm-code",
        type: "text",
        inputmode: "numeric",
        autocomplete: "off",
        maxlength: "6",
        placeholder: "000000",
        class: "code-input",
        "aria-describedby": "code-help",
      });
      const result = h("p", { class: "small", "aria-live": "polite" });
      input.addEventListener("input", () => {
        const typed = input.value.replace(/\D/g, "");
        const was = codeOk;
        codeOk = typed.length === 6 && typed === expectedCode;
        if (typed.length === 6 && !codeOk) {
          result.className = "warn-text small";
          result.textContent = "This is not the code of the computer in this link. Do not register it.";
        } else {
          result.className = "small";
          result.textContent = "";
        }
        if (codeOk !== was) render(currentAcct).catch(showError);
      });
      return h(
        "div",
        { class: "card" },
        h(
          "p",
          { class: "warn-text" },
          presence === "none"
            ? "This link does not let the page check the program on this computer."
            : "Your browser did not let this page check the program on this computer."
        ),
        h(
          "p",
          { id: "code-help" },
          `Enter the 6-digit code that ${PRODUCT.name} shows on this computer now. Read it from the program, not from a message: a code that someone sent you gives a seat to their computer.`
        ),
        codeOk
          ? h("p", { class: "ok-text" }, "The code matches the computer in this link.")
          : h("label", {}, "Code ", input),
        result
      );
    }
  }
}

function canRegister() {
  return presence === "confirmed" || ((presence === "blocked" || presence === "none") && codeOk);
}

function invalid() {
  main.replaceChildren(
    h("div", { class: "page-head" }, h("h1", {}, "Register a computer")),
    h("p", { class: "warn-text" }, "This link has no valid computer id."),
    h("p", {}, 'Open this page from the program: click "I have a license" (R on the Iron Doom title screen).')
  );
}

async function render(acct) {
  const intro = [
    h("div", { class: "page-head" }, h("span", { class: "eyebrow" }, PRODUCT.name), h("h1", {}, "Register this computer")),
    h(
      "p",
      { class: "lead" },
      `Computer ${short(device)} asks for a seat on one of your ${config.name} licenses. A license has ${config.max_devices} seats. The program checks the seat every few seconds and unlocks when it finds it.`
    ),
    presenceNote(),
  ];
  if (presence === "other") {
    main.replaceChildren(...intro);
    return;
  }
  if (!acct) {
    main.replaceChildren(
      ...intro,
      h("button", { class: "btn", onclick: () => signIn().catch(showError) }, "Connect wallet")
    );
    return;
  }
  const [licenses, seat] = await Promise.all([
    viewOf(PRODUCT, "licenses_for_owner", { account_id: acct, product: PRODUCT.key }),
    viewOf(PRODUCT, "device_license", { product: PRODUCT.key, device_id: device, version: version || "0" }),
  ]);
  if (seat) {
    main.replaceChildren(
      ...intro,
      h(
        "div",
        { class: "card" },
        h("p", { class: "ok-text" }, `This computer is registered on license #${tokenNumber(seat.token_id)} of ${seat.owner_id}.`),
        h("p", {}, "Go back to the program. It unlocks within a few seconds.")
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
  const add = h("button", { class: "btn", disabled: full || !canRegister() }, "Register this computer");
  add.addEventListener("click", () =>
    busy(add, async () => {
      await call("add_device", { token_id: l.token_id, device_id: device }, SEAT_DEPOSIT, PRODUCT.contract);
      await render(currentAcct);
    })
  );
  return h(
    "div",
    { class: "card" },
    h("h3", {}, `License #${tokenNumber(l.token_id)}`),
    h("p", {}, `Updates until ${formatDate(l.updates_until_ms)}. Seats: ${l.devices.length} of ${config.max_devices}.`),
    full
      ? h("p", { class: "muted" }, "All seats are in use. Remove a computer on ", h("a", { href: pageLink("licenses.html") }, "My licenses"), " first.")
      : add,
    h("p", { class: "muted" }, "One wallet transaction. It costs about 0.01 NEAR for storage; the contract returns the rest.")
  );
}

/** The product of this page; `config` is the same object (its terms). */
let PRODUCT;
let config;
let currentAcct = null;
if (!/^[0-9a-f]{64}$/.test(device)) {
  invalid();
} else {
  loadProduct(PRODUCT_KEY)
    .then((c) => {
      PRODUCT = c;
      config = c;
      onAccount((a) => {
        currentAcct = a;
        render(a).catch(showError);
      });
      confirmCode(device).then((c) => (expectedCode = c));
      checkPresence().then((p) => {
        presence = p;
        render(currentAcct).catch(showError);
      });
    })
    .catch(showError);
}
