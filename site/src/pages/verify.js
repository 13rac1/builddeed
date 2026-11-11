import { h, mountChrome, showError } from "../ui.js";
import { viewOf, formatDate, loadProduct } from "../near.js";

mountChrome();
const main = document.getElementById("main");
const result = h("div", {});
const table = h("div", {}, h("p", { class: "muted" }, "Loading the official builds..."));
const input = h("input", { type: "file", hidden: true });
const drop = h(
  "div",
  { class: "drop", tabindex: "0", role: "button" },
  h("p", {}, h("strong", {}, "Drop a downloaded file here"), ", or click to choose one."),
  h("p", { class: "muted" }, "The page computes the SHA-256 in your browser. The file does not leave your computer.")
);

async function sha256(file) {
  const buf = await file.arrayBuffer();
  const d = await crypto.subtle.digest("SHA-256", buf);
  return [...new Uint8Array(d)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

async function check(file) {
  result.replaceChildren(h("p", { class: "muted" }, `Hashing ${file.name}...`));
  const hash = await sha256(file);
  const m = await viewOf(product, "find_artifact", { product: product.key, sha256: hash });
  result.replaceChildren(
    h(
      "div",
      { class: "card" },
      h("h3", {}, file.name),
      h("p", { class: "mono" }, hash),
      m
        ? h(
            "div",
            {},
            h("p", { class: "ok-text" }, `Official build ${m.version} of ${product.name} for ${m.artifact.platform}.`),
            h(
              "dl",
              { class: "kv" },
              h("dt", {}, "Released"), h("dd", {}, formatDate(m.released_at_ms)),
              h("dt", {}, "Commit"), h("dd", {}, product.repo ? h("a", { class: "mono", href: `${product.repo}/commit/${m.commit}` }, m.commit) : h("span", { class: "mono" }, m.commit)),
              h("dt", {}, "Recorded file"), h("dd", {}, m.artifact.file),
              h("dt", {}, "Attestation"),
              h("dd", {}, m.artifact.attestation_url ? h("a", { href: m.artifact.attestation_url }, "Build provenance") : "none")
            )
          )
        : h("p", { class: "warn-text" }, `The license contract has no record of this file for ${product.name}. It is not an official build of this product.`)
    )
  );
}

function pick(files) {
  if (files?.[0]) check(files[0]).catch(showError);
}

drop.addEventListener("click", () => input.click());
drop.addEventListener("keydown", (e) => (e.key === "Enter" || e.key === " ") && input.click());
drop.addEventListener("dragover", (e) => {
  e.preventDefault();
  drop.classList.add("over");
});
drop.addEventListener("dragleave", () => drop.classList.remove("over"));
drop.addEventListener("drop", (e) => {
  e.preventDefault();
  drop.classList.remove("over");
  pick(e.dataTransfer.files);
});
input.addEventListener("change", () => pick(input.files));

function render() {
  main.replaceChildren(
    h("div", { class: "page-head" }, h("span", { class: "eyebrow" }, product.name), h("h1", {}, "Verify a download")),
    h(
      "p",
      { class: "lead" },
      `Each official file of ${product.name} has its SHA-256 in the license contract. A recorded hash cannot be changed or removed.`
    ),
    drop,
    input,
    result,
    h("h2", {}, "All official files"),
    table,
    h("h2", {}, "Rebuild it yourself"),
    h(
      "p",
      {},
      "The Linux and Windows files are reproducible: the same commit gives the same bytes. With Podman installed, clone the ",
      product.repo ? h("a", { href: product.repo }, "repository") : "repository",
      " and run:"
    ),
    h("pre", { class: "card mono" }, "scripts/verify.sh <version>"),
    h("p", { class: "muted" }, "The script checks out the recorded commit, builds in the pinned container image and compares each hash with the contract.")
  );
}

let product;
loadProduct()
  .then((p) => {
    product = p;
    render();
    return viewOf(p, "get_builds", { product: p.key, from_index: 0, limit: 100 });
  })
  .then((builds) => {
    const rows = builds.flatMap((b) =>
      b.artifacts.map((a) =>
        h(
          "tr",
          {},
          h("td", {}, b.version),
          h("td", {}, a.platform),
          h("td", {}, a.file),
          h("td", { class: "mono" }, a.sha256),
          h("td", {}, a.attestation_url ? h("a", { href: a.attestation_url }, "link") : "")
        )
      )
    );
    table.replaceChildren(
      rows.length
        ? h(
            "div",
            { class: "table-wrap" },
            h("table", {}, h("tr", {}, h("th", {}, "Version"), h("th", {}, "Platform"), h("th", {}, "File"), h("th", {}, "SHA-256"), h("th", {}, "Attestation")), rows)
          )
        : h("p", { class: "muted" }, "No build is recorded yet.")
    );
  })
  .catch((e) => {
    const msg = h("p", { class: "warn-text" }, `Cannot read the contract: ${e.message}`);
    if (product) table.replaceChildren(msg);
    else main.replaceChildren(msg);
  });
