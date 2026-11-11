import { h, mountChrome } from "../ui.js";
import { view, viewOf, formatNear, PRODUCT, PRODUCTS, pageLink } from "../near.js";

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

const PRODUCT_INFO = {
  demo: {
    shot: "img/checksum-nag.webp",
    alt: "BuildDeed Checksum showing its UNREGISTERED COPY screen",
    tags: ["Rust", "egui", "Linux · Windows · macOS"],
    text: "A file checksum tool. Drop a file to see its SHA-256 and its official build record. Registered: the folder check, which reports what changed since the last check.",
    pixel: false,
  },
  doom: {
    shot: "img/doom-title.webp",
    alt: "Iron Doom title screen with the shareware box: unregistered, press R to register this computer",
    tags: ["Rust", "Bevy", "GPL-3.0", "Freedoom inside"],
    text: "A Doom source port in Rust. Unregistered: E1M1 and the shareware screen. Press R on the title screen to register this computer; registered, 1-9 warp to any map.",
    pixel: true,
  },
};

const cards = document.getElementById("product-cards");
for (const p of Object.values(PRODUCTS)) {
  const info = PRODUCT_INFO[p.key];
  const price = h("span", { class: "pill" }, "…");
  cards.append(
    h(
      "article",
      { class: "card product" },
      h("div", { class: info.pixel ? "shot pixel" : "shot" }, h("img", { src: info.shot, alt: info.alt, loading: "lazy", width: 640, height: 480 })),
      h(
        "div",
        { class: "content" },
        h("h3", {}, p.name),
        h("div", { class: "tags" }, price, info.tags.map((t) => h("span", { class: "pill" }, t))),
        h("p", { class: "muted" }, info.text),
        h(
          "div",
          { class: "links" },
          h("a", { class: "btn small", href: pageLink("buy.html", p) }, "Buy a license"),
          h("a", { class: "btn small ghost", href: `${p.repo}/releases/latest` }, "Download"),
          h("a", { href: p.repo }, "Source")
        )
      )
    )
  );
  viewOf(p, "get_config")
    .then((c) => (price.textContent = `${formatNear(c.price)} NEAR · ${c.max_devices} seats`))
    .catch(() => (price.textContent = "testnet"));
}

// ----- Where the money goes -----

document.getElementById("money-product").textContent = PRODUCT.name;
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

view("get_config")
  .then((c) => {
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
      bar(`First sale of a ${c.product} license`, price, first),
      bar("A resale at 3 NEAR", resalePrice, resale)
    );
  })
  .catch((e) => document.getElementById("money-bars").replaceChildren(h("p", { class: "warn-text" }, `Cannot read the contract: ${e.message}`)));
