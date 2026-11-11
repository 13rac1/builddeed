import { h, mountChrome, busy, showOk, showError } from "../ui.js";
import { view, call, onAccount, signIn, formatNear, formatDate, parseNear, pageLink, loadProducts, tokenNumber, contractError, CONTRACT } from "../near.js";

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
  const [products, list, budget] = await Promise.all([
    loadProducts(),
    view("licenses_for_owner", { account_id: acct }),
    view("get_budget", { account_id: acct }),
  ]);
  const byKey = Object.fromEntries(products.map((p) => [p.key, p]));
  const known = list.filter((l) => byKey[l.product]);
  main.replaceChildren(
    head(),
    known.length
      ? h("div", {}, known.map((l) => card(l, byKey[l.product])))
      : h("p", {}, "This account has no license. ", h("a", { href: pageLink("buy.html") }, "Buy one"), " or look at the ", h("a", { href: pageLink("market.html") }, "market"), "."),
    budgetSection(acct, budget, products)
  );
}

// ----- Agent budget -----

/** The first deposit pays the storage of the budget record, about 0.002 NEAR. */
const MIN_FIRST_DEPOSIT = 10n ** 22n; // 0.01 NEAR

/** A NEAR amount more than zero as yoctoNEAR, or null. */
function nearAmount(text) {
  try {
    const y = parseNear(text);
    return BigInt(y) > 0n ? y : null;
  } catch {
    return null;
  }
}

function agentKeyCommand(acct) {
  return `$ near account add-key ${acct} grant-function-call-access \\
    --allowance '0.25 NEAR' \\
    --contract-account-id ${CONTRACT} \\
    --function-names buy_from_budget,add_device_from_budget \\
    autogenerate-new-keypair save-to-legacy-keychain \\
    network-config testnet sign-with-legacy-keychain send
$ jq --arg a ${acct} '{account_id: $a, public_key, private_key}' \\
    ~/.near-credentials/testnet/${acct}/ed25519_<public-key>.json > key.json
$ chmod 600 key.json`;
}

function budgetSection(acct, budget, products) {
  const names = Object.fromEntries(products.map((p) => [p.key, p.name]));
  const status = h("div", { "aria-live": "polite" });
  const fail = (e) => status.replaceChildren(h("p", { class: "warn-text", role: "alert" }, `The contract refused the call: ${contractError(e)}`));
  const warn = (msg) => status.replaceChildren(h("p", { class: "warn-text" }, msg));

  // Deposit: the amount, the highest price of one license, the products.
  // The highest price follows the checked products: the highest of their
  // prices. With a budget, it starts at the budget's value.
  const amount = h("input", { type: "text", id: "budget-amount", inputmode: "decimal", value: "1" });
  const maxPrice = h("input", { type: "text", id: "budget-max", inputmode: "decimal", placeholder: "Check a product", value: budget ? formatNear(budget.max_price) : "" });
  const boxes = products.map((p) =>
    h("label", { class: "check" }, h("input", { type: "checkbox", value: p.key, checked: budget?.products.includes(p.key) || null }), ` ${p.name}`)
  );
  const checked = () => boxes.map((b) => b.querySelector("input")).filter((i) => i.checked).map((i) => i.value);
  const prices = Object.fromEntries(products.map((p) => [p.key, BigInt(p.price)]));
  for (const b of boxes) {
    b.querySelector("input").addEventListener("change", () => {
      const keys = checked();
      maxPrice.value = keys.length ? formatNear(keys.reduce((m, k) => (prices[k] > m ? prices[k] : m), 0n).toString()) : "";
    });
  }
  const deposit = h("button", { type: "submit", class: "btn small" }, "Deposit");
  const depositForm = h(
    "form",
    { class: "form", novalidate: true },
    h(
      "div",
      { class: "row" },
      h("label", { class: "field" }, h("span", {}, "Amount (NEAR)"), amount),
      h("label", { class: "field" }, h("span", {}, "Highest price of one license (NEAR)"), maxPrice)
    ),
    h(
      "fieldset",
      { class: "field" },
      h("legend", {}, "Products the agent can buy"),
      h("div", { class: "checks" }, boxes),
      h("span", { class: "help" }, "Check 1 to 16 products. A product that is added later is not included.")
    ),
    h("div", {}, deposit)
  );
  depositForm.addEventListener("submit", (e) => {
    e.preventDefault();
    const a = nearAmount(amount.value);
    const m = nearAmount(maxPrice.value);
    if (!a) return warn("Give an amount more than zero, such as 1 or 2.5.");
    if (!budget && BigInt(a) < MIN_FIRST_DEPOSIT) return warn("The first deposit must be at least 0.01 NEAR: it also pays the storage of the budget, about 0.002 NEAR.");
    const keys = checked();
    if (!keys.length) return warn("Check at least one product that the agent can buy.");
    if (keys.length > 16) return warn("Check at most 16 products.");
    if (!m) return warn("Give a highest price more than zero, such as 0.5.");
    status.replaceChildren();
    busy(deposit, async () => {
      try {
        await call("deposit_budget", { max_price: m, products: keys }, a);
        showOk("The agent budget is updated.");
        await refresh();
      } catch (err) {
        fail(err);
      }
    });
  });

  // Withdraw: an amount, or all when the field is empty.
  const withdrawAmount = h("input", { type: "text", id: "withdraw-amount", inputmode: "decimal", placeholder: "all", "aria-label": "NEAR to withdraw (empty: all)" });
  const withdraw = h("button", { type: "button", class: "btn small ghost" }, "Withdraw");
  withdraw.addEventListener("click", () => {
    const text = withdrawAmount.value.trim();
    const a = text ? nearAmount(text) : null;
    if (text && !a) return warn("Give an amount more than zero, or leave the field empty to withdraw all.");
    if (a && BigInt(a) > BigInt(budget.balance)) return warn(`The balance is ${formatNear(budget.balance)} NEAR.`);
    status.replaceChildren();
    busy(withdraw, async () => {
      try {
        await call("withdraw_budget", a ? { amount: a } : {}, "1");
        // Without an amount, the contract removes the budget and returns null.
        showOk(a ? `${formatNear(a)} NEAR is back in your account.` : "The budget is removed. The rest of the balance and the storage of the budget record are back in your account.");
        await refresh();
      } catch (err) {
        fail(err);
      }
    });
  });

  return h(
    "section",
    { id: "agent-budget" },
    h("h2", { style: "margin-top:40px" }, "Agent budget"),
    h(
      "p",
      { class: "muted" },
      "An AI agent or a script can buy licenses and register computers for you with a function-call key on your account. Such a key cannot attach NEAR, so it pays from this budget. It cannot sell or list a license, withdraw, or change the limits: only your wallet can. ",
      h("a", { href: "agents.md" }, "Guide for agents")
    ),
    h(
      "div",
      { class: "card" },
      budget
        ? h(
            "dl",
            { class: "kv" },
            h("dt", {}, "Balance"), h("dd", {}, `${formatNear(budget.balance)} NEAR`),
            h("dt", {}, "Highest price"), h("dd", {}, `${formatNear(budget.max_price)} NEAR for one license`),
            h("dt", {}, "Products"), h("dd", {}, budget.products.map((k) => names[k] || k).join(", "))
          )
        : h("p", {}, "This account has no agent budget."),
      h("h3", { style: "margin-top:18px" }, budget ? "Add NEAR or change the limits" : "Deposit a budget"),
      depositForm,
      budget ? h("h3", { style: "margin-top:18px" }, "Withdraw") : null,
      budget ? h("p", {}, withdrawAmount, " NEAR ", withdraw) : null,
      budget ? h("p", { class: "muted small" }, "Leave the field empty to withdraw all: this also removes the budget and pays back the storage of its record. One wallet transaction with 1 yoctoNEAR attached.") : null,
      status
    ),
    h("h3", { style: "margin-top:24px" }, "Give an agent a key"),
    h(
      "p",
      {},
      "Run this with near-cli-rs on a computer that has a full-access key of your account. It adds a key that can call only buy_from_budget and add_device_from_budget on ",
      h("span", { class: "mono" }, CONTRACT),
      ". The 0.25 NEAR allowance pays only gas; licenses and seats come out of the budget."
    ),
    h("div", { class: "terminal" }, h("div", { class: "tbar" }, h("i"), h("i"), h("i")), h("pre", {}, agentKeyCommand(acct))),
    h(
      "p",
      { class: "muted small" },
      "near-cli-rs saves the new key in ~/.near-credentials/testnet/",
      acct,
      "/ed25519_<public-key>.json, and also in ~/.near-credentials/testnet/",
      acct,
      ".json when that file does not exist yet. The builddeed CLI reads the key file in that folder as it is. The file also holds a seed phrase, so for an agent on another computer the jq line writes key.json with only account_id, public_key and private_key. To stop the agent, delete the key or withdraw the budget."
    ),
    h(
      "p",
      { class: "warn-text" },
      "Copy key.json to the agent's computer, for example with scp. Never paste the key into a chat: anyone who reads the chat can spend the budget."
    )
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
    h("img", {
      class: "license-art",
      src: `nft/${l.product}.svg`,
      alt: `${p.name} license image`,
      width: "128",
      height: "128",
      loading: "lazy",
    }),
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
