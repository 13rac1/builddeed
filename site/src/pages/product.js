// The static product page /p/<key>/: the build wrote the facts; this script
// replaces them with the live page.
import { h, mountChrome } from "../ui.js";
import { loadProduct, pathProduct } from "../near.js";
import { renderProduct } from "../product-view.js";

mountChrome();
const main = document.getElementById("main");
const key = main.dataset.product || pathProduct();
loadProduct(key)
  .then((p) => renderProduct(main, p))
  .catch((e) => main.prepend(h("p", { class: "warn-text" }, `Cannot read the contract: ${e.message}. The text below is from the last build.`)));
