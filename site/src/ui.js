// Shared page parts: the header with the wallet button, the footer, and a
// small DOM helper. Data from the chain goes into the page as text only.
import "./style.css";
import {
  getConnector,
  onAccount,
  signIn,
  signOut,
  BUILDDEED_REPO,
  SDK_REPO,
  CONTRACT,
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

/** The store: for people who buy and use licenses. */
const STORE_NAV = [
  ["/", "Store"],
  ["/market/", "Used licenses"],
  ["/licenses/", "My licenses"],
  ["/verify/", "Verify a download"],
];

/** The developer area: for people who sell their software. */
const DEV_NAV = [
  ["/developers/", "Overview"],
  ["/developers/add/", "Add your product"],
  ["/developers/docs/", "Docs"],
];

/** The nav entry of a path: a product page belongs to the store front. */
function current(path) {
  const p = path.replace(/index\.html$/, "");
  if (p === "/" || p.startsWith("/p/")) return "/";
  return p;
}

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

/** Adds the header and the footer. `area` is "store" or "developers". */
export function mountChrome(area = "store") {
  const dev = area === "developers";
  const here = current(location.pathname);
  const wallet = h("div", { class: "wallet" });
  const nav = (dev ? DEV_NAV : STORE_NAV).map(([href, label]) =>
    h("a", { href, "aria-current": href === here ? "page" : null }, label)
  );
  const header = h(
    "header",
    { class: dev ? "site-header dev" : "site-header" },
    h(
      "div",
      { class: "bar" },
      h(
        "a",
        { class: "brand", href: dev ? "/developers/" : "/" },
        markup(LOGO),
        h("span", {}, "BuildDeed"),
        dev ? h("span", { class: "area" }, "for developers") : null,
        h("span", { class: "testnet", title: "This demo runs on NEAR testnet" }, "testnet")
      ),
      h("nav", { "aria-label": dev ? "Developers" : "Store" }, nav, dev ? h("a", { class: "back", href: "/" }, "Back to the store") : null),
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
          h("a", { class: "brand", href: "/" }, markup(LOGO), h("span", {}, "BuildDeed")),
          h("p", {}, "Buy software. Own the license. Resell it. Each sale pays the people who made the program.")
        ),
        h(
          "div",
          {},
          h("h4", {}, "Store"),
          h(
            "ul",
            {},
            link("/", "Store"),
            link("/market/", "Used licenses"),
            link("/licenses/", "My licenses"),
            link("/verify/", "Verify a download"),
            link("/#faq", "Questions")
          )
        ),
        h(
          "div",
          {},
          h("h4", {}, "Sell"),
          h(
            "ul",
            {},
            link("/developers/", "For developers"),
            link("/developers/add/", "Add your product"),
            link("/developers/docs/", "Docs"),
            link("/agents.md", "For agents")
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
            link(SDK_REPO, "BuildDeed SDK"),
            link(accountLink(CONTRACT), `Contract ${CONTRACT}`),
            link("https://nearhackathon.com/", "NEAR HACKATHON")
          )
        )
      ),
      h(
        "p",
        { class: "fine" },
        "Testnet demo built at NEAR HACKATHON 2026 by Hardened Minds. Testnet NEAR has no value. ",
        "BuildDeed is under the Business Source License 1.1. Each product has its own license."
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

/** Filter chips: one button for each option, `[value, label]`. The pressed
 * chip is the current value; a click calls `onSelect(value)`. */
export function chips(label, options, value, onSelect) {
  return h(
    "div",
    { class: "chips", role: "group", "aria-label": label },
    options.map(([v, text]) =>
      h("button", { type: "button", class: "chip", "aria-pressed": v === value ? "true" : "false", "data-value": v, onclick: () => onSelect(v) }, text)
    )
  );
}
