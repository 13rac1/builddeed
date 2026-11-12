// 404.html: GitHub Pages serves it at each path that has no file. A
// /p/<key>/ path of a product that was added after the last build shows
// that product from the chain; any other path is not found.
import { h, mountChrome } from "../ui.js";
import { loadProducts, pathProduct, isProductKey } from "../near.js";
import { renderProduct } from "../product-view.js";

mountChrome();
const main = document.getElementById("main");

function notFound() {
  document.title = "Not found - BuildDeed";
  main.replaceChildren(
    h("div", { class: "page-head" }, h("h1", {}, "This page does not exist")),
    h("p", {}, "The link is wrong, or the page moved. ", h("a", { href: "/" }, "Go to the store"), " or ", h("a", { href: "/verify/" }, "verify a download"), ".")
  );
}

const key = pathProduct();
if (!key || !isProductKey(key)) {
  notFound();
} else {
  main.replaceChildren(h("div", { class: "skeleton", style: "width:40%" }), h("div", { class: "skeleton" }));
  loadProducts()
    .then((list) => {
      const p = list.find((x) => x.key === key);
      if (p) renderProduct(main, p);
      else notFound();
    })
    .catch((e) => main.replaceChildren(h("p", { class: "warn-text" }, `Cannot read the contract: ${e.message}`)));
}
