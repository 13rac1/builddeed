import { h, mountChrome } from "../ui.js";
import { view, formatNear, formatDate, accountLink, REPO, PRODUCTS, pageLink } from "../near.js";

mountChrome();
document.getElementById("download").href = `${REPO}/releases/latest`;
document.getElementById("buy").href = pageLink("buy.html");

const DESCRIPTIONS = {
  demo: "A file checksum tool. Registered: the folder check, which reports the files that changed since the last check.",
  doom: "A Doom source port in Rust (GPL-3.0). Unregistered: E1M1 and the shareware screen. Registered from the title screen: every map of episode 1.",
};
document.getElementById("products").replaceChildren(
  ...Object.values(PRODUCTS).map((p) =>
    h(
      "div",
      { class: "card" },
      h("h3", {}, p.name),
      h("p", {}, DESCRIPTIONS[p.key]),
      h(
        "p",
        {},
        h("a", { class: "btn small", href: pageLink("buy.html", p) }, "Buy a license"),
        " ",
        h("a", { href: `${p.repo}/releases/latest` }, "Download"),
        " · ",
        h("a", { href: p.repo }, "Source")
      )
    )
  )
);

const terms = document.getElementById("terms");
const builds = document.getElementById("builds");

view("get_config")
  .then((c) => {
    terms.replaceChildren(
      h("h3", {}, c.product),
      h(
        "dl",
        { class: "kv" },
        h("dt", {}, "Price"), h("dd", {}, `${formatNear(c.price)} NEAR`),
        h("dt", {}, "Updates"), h("dd", {}, `${c.update_period_days} days, renew for ${formatNear(c.renewal_price)} NEAR`),
        h("dt", {}, "Resale royalty"), h("dd", {}, `${c.royalty_bps / 100}%`),
        h("dt", {}, "BuildDeed fee"), h("dd", {}, `${c.fee_bps / 100}%`),
        h("dt", {}, "Paid to"),
        h(
          "dd",
          {},
          c.payees.map((p) =>
            h("div", {}, h("a", { href: accountLink(p.account_id) }, p.account_id), ` ${p.bps / 100}%`)
          )
        )
      )
    );
  })
  .catch((e) => terms.replaceChildren(h("p", { class: "warn-text" }, `Cannot read the contract: ${e.message}`)));

view("get_builds", { from_index: 0, limit: 20 })
  .then((list) => {
    if (!list.length) {
      builds.replaceChildren(h("h3", {}, "Official builds"), h("p", { class: "muted" }, "No build is recorded yet."));
      return;
    }
    builds.replaceChildren(
      h("h3", {}, "Official builds"),
      ...list.reverse().map((b) =>
        h(
          "p",
          {},
          h("strong", {}, b.version),
          ` released ${formatDate(b.released_at_ms)}, commit `,
          h("a", { href: `${REPO}/commit/${b.commit}`, class: "mono" }, b.commit.slice(0, 12)),
          `, ${b.artifacts.length} files. `,
          h("a", { href: pageLink("verify.html") }, "Hashes")
        )
      )
    );
  })
  .catch((e) => builds.replaceChildren(h("p", { class: "warn-text" }, `Cannot read the builds: ${e.message}`)));
