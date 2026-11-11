// Shared page parts: the header with the wallet button, the footer, and a
// small DOM helper. Data from the chain goes into the page as text only.
import "./style.css";
import {
  getConnector,
  onAccount,
  signIn,
  signOut,
  BUILDDEED_REPO,
  IRON_DOOM_REPO,
  CONTRACT,
  PRODUCT_KEY,
  loadProducts,
  pageLink,
  accountLink,
  contractError,
} from "./near.js";

/** h("a", { href: "..." }, "text", child) -> element */
export function h(tag, attrs = {}, ...children) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v === null || v === undefined || v === false) continue;
    if (k.startsWith("on")) el.addEventListener(k.slice(2), v);
    else if (k === "class") el.className = v;
    else el.setAttribute(k, v === true ? "" : v);
  }
  for (const c of children.flat()) {
    if (c === null || c === undefined || c === false) continue;
    el.append(c instanceof Node ? c : document.createTextNode(String(c)));
  }
  return el;
}

const NAV = [
  ["index.html", "Home"],
  ["buy.html", "Buy"],
  ["licenses.html", "My licenses"],
  ["market.html", "Market"],
  ["verify.html", "Verify"],
  ["developers.html", "Developers"],
];

/** An SVG or other fixed markup from this file (never chain data). */
export function markup(html) {
  const t = document.createElement("template");
  t.innerHTML = html.trim();
  return t.content.firstChild;
}

/** The BuildDeed mark: a deed with a seal. */
export const LOGO = `<svg viewBox="0 0 32 32" aria-hidden="true">
  <path d="M7 2h12l6 6v18a4 4 0 0 1-4 4H7a4 4 0 0 1-4-4V6a4 4 0 0 1 4-4z" fill="var(--accent)"/>
  <path d="M19 2v4a2 2 0 0 0 2 2h4" fill="none" stroke="var(--accent-ink)" stroke-width="1.5" opacity=".55"/>
  <path d="M8 12h9M8 16.5h7" stroke="var(--accent-ink)" stroke-width="2" stroke-linecap="round"/>
  <circle cx="21.5" cy="23.5" r="6.5" fill="var(--stamp)"/>
  <path d="m18.7 23.6 2 2 3.6-3.9" stroke="#fff" stroke-width="2" fill="none" stroke-linecap="round" stroke-linejoin="round"/>
</svg>`;

export function mountChrome() {
  const here = location.pathname.split("/").pop() || "index.html";
  const wallet = h("div", { class: "wallet" });
  // The products come from the contract; the links appear when they load.
  const switcher = h("div", { class: "products", role: "group", "aria-label": "Product" });
  const footerProducts = h("ul", {});
  const header = h(
    "header",
    { class: "site-header" },
    h(
      "div",
      { class: "bar" },
      h(
        "a",
        { class: "brand", href: pageLink("index.html") },
        markup(LOGO),
        h("span", {}, "BuildDeed"),
        h("span", { class: "testnet", title: "This demo runs on NEAR testnet" }, "testnet")
      ),
      h(
        "nav",
        { "aria-label": "Site" },
        NAV.map(([href, label]) =>
          h("a", { href: pageLink(href), "aria-current": href === here ? "page" : null }, label)
        )
      ),
      switcher,
      wallet
    )
  );
  document.body.prepend(header);

  const link = (href, text) => h("li", {}, h("a", { href }, text));
  document.body.append(
    h(
      "footer",
      { class: "site-footer" },
      h(
        "div",
        { class: "cols" },
        h(
          "div",
          {},
          h("a", { class: "brand", href: "index.html" }, markup(LOGO), h("span", {}, "BuildDeed")),
          h(
            "p",
            {},
            "Software licenses as NFTs on NEAR, in one collection for all products. The license gives you the verified official build; each sale pays the people who made it."
          )
        ),
        h(
          "div",
          {},
          h("h4", {}, "Products"),
          footerProducts
        ),
        h(
          "div",
          {},
          h("h4", {}, "Use"),
          h(
            "ul",
            {},
            link(pageLink("licenses.html"), "My licenses"),
            link(pageLink("market.html"), "Market"),
            link(pageLink("verify.html"), "Verify a download"),
            link("developers.html", "For developers"),
            link("index.html#faq", "Questions")
          )
        ),
        h(
          "div",
          {},
          h("h4", {}, "Source"),
          h(
            "ul",
            {},
            link(BUILDDEED_REPO, "BuildDeed on GitHub"),
            link(IRON_DOOM_REPO, "Iron Doom on GitHub"),
            link(accountLink(CONTRACT), `License collection ${CONTRACT}`),
            link("https://nearhackathon.com/", "NEAR HACKATHON")
          )
        )
      ),
      h(
        "p",
        { class: "fine" },
        "Testnet demo built at NEAR HACKATHON 2026 by Hardened Minds. Testnet NEAR has no value. ",
        "BuildDeed is under the Business Source License 1.1; Iron Doom is under the GPL-3.0."
      )
    )
  );

  onAccount((acct) => {
    wallet.replaceChildren(
      acct
        ? h(
            "span",
            { class: "acct" },
            h("a", { href: accountLink(acct) }, acct),
            h("button", { class: "link", onclick: () => signOut() }, "Sign out")
          )
        : h("button", { class: "btn small", onclick: () => signIn().catch(showError) }, "Connect wallet")
    );
  });
  getConnector().catch((e) => console.error("wallet list", e));

  // My licenses and the market show all products, and the program sets the
  // product of the register page: these pages have no switcher.
  const noSwitcher = ["licenses.html", "market.html", "register.html"].includes(here);
  loadProducts()
    .then((products) => {
      footerProducts.replaceChildren(...products.map((p) => link(pageLink("buy.html", p), p.name)));
      if (noSwitcher) return;
      switcher.replaceChildren(
        ...products.map((p) =>
          h("a", { href: pageLink(here, p), "aria-current": p.key === PRODUCT_KEY ? "true" : null }, p.name)
        )
      );
    })
    .catch(() => {});
}

export function showError(e) {
  const msg = contractError(e);
  const box = h("div", { class: "toast error", role: "alert" }, msg);
  document.body.append(box);
  setTimeout(() => box.remove(), 8000);
}

export function showOk(msg) {
  const box = h("div", { class: "toast ok", role: "status" }, msg);
  document.body.append(box);
  setTimeout(() => box.remove(), 6000);
}

/** Runs an async action and shows its error; disables the button meanwhile. */
export async function busy(button, f) {
  const label = button.textContent;
  button.disabled = true;
  button.textContent = "Waiting for the wallet...";
  try {
    return await f();
  } catch (e) {
    showError(e);
  } finally {
    button.disabled = false;
    button.textContent = label;
  }
}
