import { h, mountChrome } from "../ui.js";
import { formatNear, loadProducts, pageLink, PRODUCT_KEY } from "../near.js";

mountChrome();
document.getElementById("verify-link").href = pageLink("verify.html");

// ----- The shareware dialog in the hero -----

const dialog = document.getElementById("demo-dialog");
const status = document.getElementById("demo-status");
const text = document.getElementById("demo-text");
const cont = document.getElementById("demo-continue");
let left = 5;
const countdown = setInterval(() => {
  left -= 1;
  cont.textContent = left > 0 ? `Continue (${left})` : "Continue";
  if (left <= 0) {
    cont.disabled = false;
    clearInterval(countdown);
  }
}, 1000);
document.getElementById("demo-register").addEventListener("click", () => {
  dialog.classList.add("registered");
  status.textContent = "REGISTERED TO ALICE.TESTNET";
  text.textContent = "Thank you. The folder check is on, and this screen is gone. Your license has 2 free seats.";
});
document.getElementById("demo-buy").addEventListener("click", () => {
  location.href = pageLink("buy.html");
});
cont.addEventListener("click", () => {
  text.textContent = "You can use it unregistered. The nag comes back at the next start.";
});

// ----- Product cards -----

const cards = document.getElementById("product-cards");

function productCard(p) {
  // A product without local display data gets a card without a picture.
  const tags = [`${formatNear(p.price)} NEAR · ${p.max_devices} seats`, ...(p.tags || [])];
  const github = p.repo && p.repo.startsWith("https://github.com/");
  return h(
    "article",
    { class: "card product" },
    p.shot
      ? h("div", { class: p.pixel ? "shot pixel" : "shot" }, h("img", { src: p.shot, alt: p.alt, loading: "lazy", width: 640, height: 480 }))
      : null,
    h(
      "div",
      { class: "content" },
      h("h3", {}, p.name),
      h("div", { class: "tags" }, tags.map((t) => h("span", { class: "pill" }, t))),
      h("p", { class: "muted" }, p.text || p.description),
      h(
        "div",
        { class: "links" },
        h("a", { class: "btn small", href: pageLink("buy.html", p) }, "Buy a license"),
        github ? h("a", { class: "btn small ghost", href: `${p.repo}/releases/latest` }, "Download") : null,
        p.repo ? h("a", { href: p.repo }, github ? "Source" : "Product page") : null
      )
    )
  );
}

// ----- Where the money goes -----

const COLORS = ["var(--seg-3)", "var(--seg-1)", "var(--seg-2)", "var(--seg-4)", "#7c3aed", "#db2777"];

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

loadProducts()
  .then((products) => {
    cards.replaceChildren(...products.map(productCard));
    const c = products.find((p) => p.key === PRODUCT_KEY) || products[0];
    document.getElementById("money-product").textContent = c.name;
    const price = BigInt(c.price);
    const fee = (price * BigInt(c.fee_bps)) / 10000n;
    const rest = price - fee;
    const first = [
      { label: "BuildDeed fee", amount: fee },
      ...c.payees.map((p) => ({ label: p.account_id.split(".")[0], amount: (rest * BigInt(p.bps)) / 10000n })),
    ];
    const resalePrice = 3n * 10n ** 24n;
    const rFee = (resalePrice * BigInt(c.fee_bps)) / 10000n;
    const royalty = (resalePrice * BigInt(c.royalty_bps)) / 10000n;
    const resale = [
      { label: "BuildDeed fee", amount: rFee },
      ...c.payees.map((p) => ({ label: `${p.account_id.split(".")[0]} royalty`, amount: (royalty * BigInt(p.bps)) / 10000n })),
      { label: "Seller", amount: resalePrice - rFee - royalty },
    ];
    document.getElementById("money-bars").replaceChildren(
      bar(`First sale of a ${c.name} license`, price, first),
      bar("A resale at 3 NEAR", resalePrice, resale)
    );
  })
  .catch((e) => {
    const msg = `Cannot read the contract: ${e.message}`;
    cards.replaceChildren(h("p", { class: "warn-text" }, msg));
    document.getElementById("money-bars").replaceChildren(h("p", { class: "warn-text" }, msg));
  });
