// Add your product: the add_product form of the developer area.
import { h, mountChrome, busy, showOk } from "../ui.js";
import { CONTRACT, view, call, onAccount, parseNear, formatNear, contractError, productPath } from "../near.js";

mountChrome("developers");
const main = document.getElementById("main");

// The same limits as the contract (contract/src/lib.rs, check_terms).
const MAX_NAME = 64;
const MAX_DESCRIPTION = 280;
const MAX_URL = 512;
const MAX_PAYEES = 16;
const MAX_DEVICES = 16;
const MAX_DAYS = 36500;
/** The product record takes about 0.005 NEAR; the rest is the storage balance. */
const MIN_DEPOSIT = 10n ** 22n; // 0.01 NEAR

/** The BuildDeed fee in basis points, from get_settings. */
let feeBps = 300;

// ----- Input checks -----

const bytes = (s) => new TextEncoder().encode(s).length;

/** 2 to 32 characters a-z, 0-9 and -, starting with a letter or digit. */
function isProductKey(k) {
  return k.length >= 2 && k.length <= 32 && /^[a-z0-9][a-z0-9-]*$/.test(k);
}

/** A NEAR account id: 2 to 64 characters, parts of a-z 0-9 _ - between dots. */
function isAccountId(a) {
  return a.length >= 2 && a.length <= 64 && /^(([a-z\d]+[-_])*[a-z\d]+\.)*([a-z\d]+[-_])*[a-z\d]+$/.test(a);
}

/** "10" or "12.5" (percent, at most 2 decimals) -> basis points, or null. */
function percentToBps(text) {
  const m = text.trim().match(/^(\d{1,3})(?:\.(\d{1,2}))?$/);
  if (!m) return null;
  return Number(m[1]) * 100 + Number((m[2] || "").padEnd(2, "0"));
}

/** A whole number from min to max, or null. */
function wholeNumber(text, min, max) {
  if (!/^\d+$/.test(text.trim())) return null;
  const n = Number(text.trim());
  return n >= min && n <= max ? n : null;
}

/** A NEAR amount more than zero as yoctoNEAR, or null. */
function nearAmount(text) {
  try {
    const y = parseNear(text);
    return BigInt(y) > 0n ? y : null;
  } catch {
    return null;
  }
}

// ----- The form -----

function field(id, label, input, help) {
  const error = h("span", { class: "field-error", id: `${id}-error`, "aria-live": "polite" });
  input.id = id;
  input.setAttribute("aria-describedby", `${id}-help ${id}-error`);
  const el = h("label", { class: "field" }, h("span", {}, label), input, help ? h("span", { class: "help", id: `${id}-help` }, help) : null, error);
  return { el, input, error };
}

const text = (attrs = {}) => h("input", { type: "text", autocomplete: "off", ...attrs });

const f = {
  key: field("p-key", "Product key", text({ placeholder: "my-app", maxlength: "32", spellcheck: "false" }), "2 to 32 characters: a-z, 0-9 and -. It starts with a letter or digit. It is in your token ids (my-app:1) and links. You cannot change it."),
  name: field("p-name", "Name", text({ placeholder: "My App" }), `1 to ${MAX_NAME} bytes.`),
  description: field("p-description", "Description", h("textarea", { rows: "3" }), `One or two sentences for the website. At most ${MAX_DESCRIPTION} bytes.`),
  url: field("p-url", "Source or product page (optional)", h("input", { type: "url", placeholder: "https://github.com/you/my-app" }), `An https URL, at most ${MAX_URL} characters.`),
  price: field("p-price", "Price (NEAR)", text({ inputmode: "decimal", value: "5" }), "The price of a new license. It must cover the storage of a license token, about 0.01 NEAR."),
  renewal: field("p-renewal", "Renewal price (NEAR)", text({ inputmode: "decimal", value: "2" }), "The price to add one update period. It is also the lowest resale price."),
  days: field("p-days", "Update period (days)", text({ inputmode: "numeric", value: "365" }), `A license covers the builds released in this period. 1 to ${MAX_DAYS}.`),
  royalty: field("p-royalty", "Royalty (%)", text({ inputmode: "decimal", value: "10" }), "The part of each resale that goes to your payees. You cannot change it later."),
  seats: field("p-seats", "Seats per license", text({ inputmode: "numeric", value: "3" }), `The number of computers on one license. 1 to ${MAX_DEVICES}. You cannot change it later.`),
  deposit: field("p-deposit", "Storage deposit (NEAR)", text({ inputmode: "decimal", value: "0.5" }), "The product record takes about 0.005 NEAR. The rest is the storage balance of your product: add_build takes the storage of each build record from it. 0.5 NEAR is enough for many releases."),
};

// Payees: an account and a share in percent on each row.
const payeeRows = h("div", { class: "form" });
const payeeError = h("span", { class: "field-error", "aria-live": "polite" });
const addPayee = h("button", { type: "button", class: "btn small ghost" }, "Add a payee");

function payeeRow(account = "", share = "") {
  const acct = text({ placeholder: "alice.testnet", value: account, spellcheck: "false", "aria-label": "Payee account" });
  const pct = text({ inputmode: "decimal", value: share, placeholder: "%", "aria-label": "Share in percent" });
  const remove = h("button", { type: "button", class: "link" }, "Remove");
  const row = h("div", { class: "payee" }, acct, pct, remove);
  row.inputs = { acct, pct };
  remove.addEventListener("click", () => {
    if (payeeRows.children.length > 1) row.remove();
    updatePayeeButtons();
  });
  return row;
}

function updatePayeeButtons() {
  const rows = [...payeeRows.children];
  addPayee.disabled = rows.length >= MAX_PAYEES;
  rows.forEach((r) => (r.querySelector("button").disabled = rows.length === 1));
}

addPayee.addEventListener("click", () => {
  if (payeeRows.children.length < MAX_PAYEES) payeeRows.append(payeeRow());
  updatePayeeButtons();
});
payeeRows.append(payeeRow("", "100"));
updatePayeeButtons();

/** Checks the form. Returns the add_product arguments and the deposit, or
 * null after it shows the errors. */
function readForm() {
  let ok = true;
  const fail = (fld, msg) => {
    fld.error.textContent = msg;
    ok = false;
  };
  Object.values(f).forEach((fld) => (fld.error.textContent = ""));
  payeeError.textContent = "";

  const key = f.key.input.value.trim();
  if (!isProductKey(key)) fail(f.key, "Use 2 to 32 characters a-z, 0-9 and -, and start with a letter or digit.");

  const name = f.name.input.value.trim();
  if (bytes(name) < 1 || bytes(name) > MAX_NAME) fail(f.name, `The name must have 1 to ${MAX_NAME} bytes.`);

  const description = f.description.input.value.trim();
  if (bytes(description) > MAX_DESCRIPTION) fail(f.description, `The description has ${bytes(description)} bytes. The limit is ${MAX_DESCRIPTION}.`);

  const url = f.url.input.value.trim();
  if (url && (!url.startsWith("https://") || url.length > MAX_URL)) fail(f.url, `The URL must start with https:// and have at most ${MAX_URL} characters.`);

  const price = nearAmount(f.price.input.value);
  if (!price) fail(f.price, "Give an amount more than zero, such as 5 or 2.5.");
  const renewal = nearAmount(f.renewal.input.value);
  if (!renewal) fail(f.renewal, "Give an amount more than zero, such as 2 or 0.5.");

  const days = wholeNumber(f.days.input.value, 1, MAX_DAYS);
  if (days === null) fail(f.days, `Give a whole number from 1 to ${MAX_DAYS}.`);

  const royalty = percentToBps(f.royalty.input.value);
  if (royalty === null || royalty > 10000) fail(f.royalty, "Give a percent such as 10 or 12.5 (at most 2 decimals).");
  else if (royalty + feeBps > 10000) fail(f.royalty, `The royalty and the ${feeBps / 100}% fee together must be at most 100%.`);

  const seats = wholeNumber(f.seats.input.value, 1, MAX_DEVICES);
  if (seats === null) fail(f.seats, `Give a whole number from 1 to ${MAX_DEVICES}.`);

  const deposit = nearAmount(f.deposit.input.value);
  if (!deposit || BigInt(deposit) < MIN_DEPOSIT) fail(f.deposit, `Attach at least ${formatNear(MIN_DEPOSIT.toString())} NEAR.`);

  const payees = [];
  const seen = new Set();
  let total = 0;
  for (const row of payeeRows.children) {
    const account_id = row.inputs.acct.value.trim();
    const bps = percentToBps(row.inputs.pct.value);
    if (!isAccountId(account_id)) {
      payeeError.textContent = `"${account_id}" is not a NEAR account id.`;
      ok = false;
      break;
    }
    if (seen.has(account_id)) {
      payeeError.textContent = `${account_id} is in the list more than once.`;
      ok = false;
      break;
    }
    if (bps === null || bps < 1) {
      payeeError.textContent = `Give the share of ${account_id} as a percent more than zero, with at most 2 decimals.`;
      ok = false;
      break;
    }
    seen.add(account_id);
    total += bps;
    payees.push({ account_id, bps });
  }
  if (ok && total !== 10000) {
    payeeError.textContent = `The shares add up to ${total / 100}%. They must add up to 100%.`;
    ok = false;
  }

  if (!ok) return null;
  return {
    args: {
      key,
      terms: {
        name,
        description,
        url: url || null,
        price,
        renewal_price: renewal,
        update_period_days: days,
        royalty_bps: royalty,
        payees,
        max_devices: seats,
      },
    },
    deposit,
  };
}

const status = h("div", { "aria-live": "polite" });
const submit = h("button", { type: "submit", class: "btn" }, "Add the product");
const form = h(
  "form",
  { class: "card form", novalidate: true },
  f.key.el,
  f.name.el,
  f.description.el,
  f.url.el,
  h("div", { class: "row" }, f.price.el, f.renewal.el),
  h("div", { class: "row" }, f.days.el, f.royalty.el, f.seats.el),
  h(
    "div",
    { class: "field" },
    h("span", {}, "Payees"),
    h("span", { class: "help" }, "You and your contributors: 1 to 16 NEAR accounts. Each first sale, renewal and royalty is split by these shares. The shares must add up to 100%. You can change the payees later with set_payees."),
    payeeRows,
    payeeError,
    h("div", {}, addPayee)
  ),
  f.deposit.el,
  h("p", { class: "muted small" }, "One wallet transaction from the account that becomes the developer of the product. Only that account can record builds and change the prices and payees."),
  submit,
  status
);

form.addEventListener("submit", (e) => {
  e.preventDefault();
  status.replaceChildren();
  const input = readForm();
  if (!input) {
    status.replaceChildren(h("p", { class: "warn-text" }, "Correct the fields in red."));
    return;
  }
  busy(submit, async () => {
    try {
      const existing = await view("get_product", { key: input.args.key });
      if (existing) {
        f.key.error.textContent = "A product with this key exists. Use another key.";
        status.replaceChildren(h("p", { class: "warn-text" }, "Correct the fields in red."));
        return;
      }
      await call("add_product", input.args, input.deposit);
      showOk("The product is added.");
      status.replaceChildren(
        h(
          "p",
          { class: "ok-text" },
          `${input.args.terms.name} is in the store. Its page: `,
          h("a", { href: productPath(input.args.key) }, productPath(input.args.key)),
          ". The static page appears after the next build of the website; until then the same address shows it from the chain."
        )
      );
    } catch (err) {
      status.replaceChildren(h("p", { class: "warn-text", role: "alert" }, `The contract refused the call: ${contractError(err)}`));
    }
  });
});

// Put the signed-in account in the first payee row while it is empty.
onAccount((acct) => {
  const first = payeeRows.firstElementChild?.inputs.acct;
  if (acct && first && !first.value) first.value = acct;
});

// ----- The page -----

main.replaceChildren(
  h(
    "div",
    { class: "page-head" },
    h("h1", {}, "Add your product"),
    h(
      "p",
      { class: "lead" },
      "One call adds your product to the store. Your account becomes its developer: only it can record builds and change the prices and payees."
    )
  ),
  h(
    "p",
    { class: "muted" },
    "This form calls add_product on ",
    h("span", { class: "mono" }, CONTRACT),
    ". This is NEAR testnet: the NEAR has no value."
  ),
  form,
  h(
    "div",
    { class: "card next" },
    h("h2", {}, "Next: set up your releases"),
    h("p", {}, "Give your release workflow a key that can only record builds, and keep the storage balance of your product above zero. ", h("a", { href: "/developers/docs/#releases" }, "Read the release steps"), ".")
  )
);

view("get_settings")
  .then((s) => (feeBps = s.fee_bps))
  .catch(() => {});
