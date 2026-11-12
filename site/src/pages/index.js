// The store front: the app grid from the chain. The build wrote the same
// cards into the HTML; this script replaces them with live prices and adds
// products that were added after the build.
import { h, mountChrome, licenseImage } from "../ui.js";
import { loadProducts, formatNear, productPath } from "../near.js";

mountChrome();

/** One app card. The whole card is a link to the product page. */
function appCard(p) {
  return h(
    "a",
    { class: "app-card", href: productPath(p.key) },
    p.shot
      ? h("span", { class: p.pixel ? "app-media pixel" : "app-media" }, h("img", { src: p.shot, alt: p.alt || p.name, width: 640, height: 480, loading: "lazy" }))
      : h("span", { class: "app-media license" }, licenseImage(p.key, { alt: `${p.name} license image`, width: 512, height: 512, loading: "lazy" })),
    h(
      "span",
      { class: "app-body" },
      h("span", { class: "app-name" }, p.name),
      h("span", { class: "app-desc" }, p.description || ""),
      h(
        "span",
        { class: "app-meta" },
        h("span", { class: "app-price" }, `${formatNear(p.price)} NEAR`),
        (p.platforms || []).length ? h("span", { class: "muted" }, p.platforms.join(" · ")) : null
      ),
      h("span", { class: "btn small app-get" }, "Get")
    )
  );
}

const grid = document.getElementById("app-grid");
loadProducts()
  .then((list) => grid.replaceChildren(...list.map(appCard)))
  .catch((e) => grid.append(h("p", { class: "warn-text" }, `Cannot read the contract: ${e.message}. The prices above are from the last build.`)));
