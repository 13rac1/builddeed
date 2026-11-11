// Shared page parts: the header with the wallet button, the footer, and a
// small DOM helper. Data from the chain goes into the page as text only.
import "./style.css";
import {
  getConnector,
  onAccount,
  signIn,
  signOut,
  BUILDDEED_REPO,
  CONTRACT,
  PRODUCT,
  PRODUCTS,
  pageLink,
  accountLink,
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
  ["verify.html", "Verify a download"],
];

export function mountChrome() {
  const here = location.pathname.split("/").pop() || "index.html";
  const wallet = h("div", { class: "wallet" });
  const header = h(
    "header",
    { class: "site-header" },
    h(
      "a",
      { class: "brand", href: "index.html" },
      h("span", { class: "brand-mark", "aria-hidden": "true" }, "BD"),
      h("span", {}, "BuildDeed")
    ),
    h(
      "nav",
      {},
      NAV.map(([href, label]) =>
        h("a", { href: pageLink(href), "aria-current": href === here ? "page" : null }, label)
      )
    ),
    h(
      "div",
      { class: "products", role: "group", "aria-label": "Product" },
      Object.values(PRODUCTS).map((p) =>
        h(
          "a",
          { href: pageLink(here, p), class: "pill", "aria-current": p.key === PRODUCT.key ? "true" : null },
          p.name
        )
      )
    ),
    wallet
  );
  document.body.prepend(header);

  document.body.append(
    h(
      "footer",
      { class: "site-footer" },
      h(
        "p",
        {},
        "Built at NEAR HACKATHON 2026 by Hardened Minds. Testnet demo: the license contract of ",
        PRODUCT.name,
        " is ",
        h("a", { href: accountLink(CONTRACT) }, CONTRACT),
        ". BuildDeed source: ",
        h("a", { href: BUILDDEED_REPO }, "github.com/13rac1/builddeed"),
        " (Business Source License 1.1)."
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
  const msg = e?.message || String(e);
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
