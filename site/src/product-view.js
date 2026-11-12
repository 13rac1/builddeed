// The product page: /p/<key>/ (a static page from the build), and 404.html
// for a product that was added after the last build. This module builds the
// whole page from live data and replaces the static text.
import { h, busy, showOk, markup, licenseImage } from "./ui.js";
import {
  viewOf,
  call,
  onAccount,
  formatNear,
  formatDate,
  tokenNumber,
  accountLink,
  marketLink,
  verifyLink,
  termLines,
  FAUCET,
} from "./near.js";

/** The platforms of the download list, in this order. */
const PLATFORMS = [
  ["linux-x86_64", "Linux"],
  ["windows-x86_64", "Windows"],
  ["macos-aarch64", "macOS"],
];

const COLORS = ["var(--seg-3)", "var(--seg-1)", "var(--seg-2)", "var(--seg-4)", "#7c3aed", "#db2777"];

const SHIELD = `<svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3 4 6v6c0 4.5 3.4 8.2 8 9 4.6-.8 8-4.5 8-9V6z"/><path d="m8.5 12 2.5 2.5 4.5-5"/></svg>`;

function bar(title, total, parts) {
  const t = Number(total);
  return h(
    "div",
    {},
    h("div", { class: "split-label" }, h("span", {}, title), h("span", {}, `${formatNear(total)} NEAR`)),
    h(
      "div",
      { class: "split-bar", role: "img", "aria-label": parts.map((p) => `${p.label} ${formatNear(p.amount)} NEAR`).join(", ") },
      parts.map((p, i) => {
        const share = (Number(p.amount) / t) * 100;
        return h(
          "div",
          { style: `width:${share}%;background:${COLORS[i % COLORS.length]}`, title: `${p.label}: ${formatNear(p.amount)} NEAR` },
          share >= 12 ? `${Math.round(share)}%` : ""
        );
      })
    ),
    h(
      "div",
      { class: "legend" },
      parts.map((p, i) =>
        h("span", {}, h("i", { style: `background:${COLORS[i % COLORS.length]}` }), `${p.label} `, h("b", {}, formatNear(p.amount)))
      )
    )
  );
}

/** Where the money of a first sale and of a resale goes, for this product. */
function moneyBars(p) {
  const price = BigInt(p.price);
  const fee = (price * BigInt(p.fee_bps)) / 10000n;
  const rest = price - fee;
  const name = (a) => a.split(".")[0];
  const first = [
    { label: "BuildDeed fee", amount: fee },
    ...p.payees.map((x) => ({ label: name(x.account_id), amount: (rest * BigInt(x.bps)) / 10000n })),
  ];
  // A resale example at the price of a new license.
  const rFee = (price * BigInt(p.fee_bps)) / 10000n;
  const royalty = (price * BigInt(p.royalty_bps)) / 10000n;
  const resale = [
    { label: "BuildDeed fee", amount: rFee },
    ...p.payees.map((x) => ({ label: `${name(x.account_id)} royalty`, amount: (royalty * BigInt(x.bps)) / 10000n })),
    { label: "Seller", amount: price - rFee - royalty },
  ];
  return h(
    "div",
    { class: "card money" },
    bar("A new license", price, first),
    bar(`A resale at ${formatNear(price)} NEAR`, price, resale),
    h("p", { class: "muted small" }, "The contract pays everyone in the same transaction. The storage of a new license token (about 0.01 NEAR) comes out of the first sale before the split.")
  );
}

/** Fills `main` with the page of product `p` (a product object from loadProducts). */
export function renderProduct(main, p) {
  document.title = `${p.name} - BuildDeed`;
  const used = h("span", { class: "used" });
  const owned = h("div", {});
  const downloads = h("div", { class: "card" }, h("div", { class: "skeleton" }), h("div", { class: "skeleton" }));

  /** The signed-in account, or null. */
  let last = null;
  const buy = h("button", { class: "btn" }, `Buy for ${formatNear(p.price)} NEAR`);
  buy.addEventListener("click", () =>
    busy(buy, async () => {
      await call("buy", { product: p.key }, p.price, p.contract);
      showOk(`You own a ${p.name} license. Open the program and register this computer.`);
      await showOwned(last);
    })
  );

  main.replaceChildren(
    h(
      "article",
      { class: "product-hero" },
      h(
        "div",
        { class: "product-media" },
        p.shot ? h("img", { class: p.pixel ? "shot pixel" : "shot", src: p.shot, alt: p.alt || p.name, width: 640, height: 480 }) : null,
        licenseImage(p.key, { class: "license-badge", alt: `${p.name} license image`, width: 96, height: 96 })
      ),
      h(
        "div",
        { class: "product-info" },
        h("h1", {}, p.name),
        h("p", { class: "lead" }, p.description || p.text || ""),
        h("p", { class: "price" }, h("strong", {}, `${formatNear(p.price)} NEAR`), " ", h("span", { class: "muted" }, "for a license, on NEAR testnet")),
        h("div", { class: "tags" }, (p.platforms || []).map((x) => h("span", { class: "pill" }, x)), (p.tags || []).map((x) => h("span", { class: "pill" }, x))),
        h("div", { class: "btn-row" }, buy, used),
        h("p", { class: "muted small" }, "Testnet NEAR has no value. Get some free from the ", h("a", { href: FAUCET }, "faucet"), "."),
        owned
      )
    ),
    h("section", {}, h("h2", {}, "Downloads"), downloads),
    h("section", {}, h("h2", {}, "What you get"), h("ul", { class: "terms" }, termLines(p).map((t) => h("li", {}, t)))),
    h(
      "section",
      {},
      h("h2", {}, "About"),
      p.text ? h("p", {}, p.text) : null,
      h(
        "dl",
        { class: "kv" },
        h("dt", {}, "Developer"), h("dd", {}, h("a", { href: accountLink(p.developer) }, p.developer)),
        p.repo ? [h("dt", {}, "Source"), h("dd", {}, h("a", { href: p.repo }, p.repo))] : null,
        h("dt", {}, "Licenses sold"), h("dd", {}, String(p.licenses_sold)),
        h("dt", {}, "Product key"), h("dd", { class: "mono" }, p.key)
      )
    ),
    h("section", {}, h("h2", {}, "Where the money goes"), moneyBars(p))
  );

  async function showOwned(acct) {
    if (!acct) {
      owned.replaceChildren();
      return;
    }
    const list = await viewOf(p, "licenses_for_owner", { account_id: acct, product: p.key });
    owned.replaceChildren(
      list.length
        ? h(
            "div",
            { class: "owned" },
            h("strong", {}, list.length === 1 ? "You own this license" : `You own ${list.length} licenses`),
            list.map((l) => h("span", { class: "muted small" }, ` #${tokenNumber(l.token_id)}, updates until ${formatDate(l.updates_until_ms)}.`)),
            " ",
            h("a", { href: "/licenses/" }, "Manage")
          )
        : ""
    );
  }
  onAccount((a) => {
    last = a;
    showOwned(a).catch(() => {});
  });

  viewOf(p, "get_listings", { product: p.key, from_index: 0, limit: 100 })
    .then((ls) => {
      if (!ls.length) return;
      const low = ls.map((l) => BigInt(l.listed_price)).reduce((a, b) => (b < a ? b : a));
      used.replaceChildren(h("a", { href: marketLink(p.key) }, `Used from ${formatNear(low.toString())} NEAR`));
    })
    .catch(() => {});

  Promise.all(PLATFORMS.map(([platform]) => viewOf(p, "latest_build", { product: p.key, platform })))
    .then((found) => {
      const rows = PLATFORMS.map(([, label], i) => [label, found[i]]).filter(([, m]) => m);
      downloads.replaceChildren(
        rows.length
          ? h(
              "ul",
              { class: "downloads" },
              rows.map(([label, m]) =>
                h(
                  "li",
                  {},
                  h("strong", {}, label),
                  " ",
                  m.artifact.url ? h("a", { href: m.artifact.url }, m.artifact.file) : h("span", { class: "mono" }, m.artifact.file),
                  h("span", { class: "muted small" }, ` version ${m.version}, ${formatDate(m.released_at_ms)}`)
                )
              )
            )
          : h("p", { class: "muted" }, "No build is recorded yet."),
        rows.length
          ? h(
              "p",
              { class: "verified" },
              markup(SHIELD),
              h("span", {}, h("strong", {}, "Verified build. "), "Built from the public source in a pinned container, attested by GitHub, and its SHA-256 recorded on chain. "),
              h("a", { href: verifyLink(p.key) }, "Check a file")
            )
          : null
      );
    })
    .catch((e) => downloads.replaceChildren(h("p", { class: "warn-text" }, `Cannot read the builds: ${e.message}`)));
}
