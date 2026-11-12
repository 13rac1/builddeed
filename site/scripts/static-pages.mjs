// Build-time pages from the chain. vite.config.js calls these functions:
// one get_products request gives a static page for each product at
// /p/<key>/index.html, the static app grid of the home page, and the
// sitemap. The facts are in the HTML, so a reader without JavaScript (an
// agent, a crawler) gets them; the page script then adds live data.
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import {
  CONTRACT,
  RPC,
  SITE,
  withExtras,
  formatNear,
  productPath,
  nftImage,
  termLines,
  isProductKey,
} from "../src/catalog.js";

/** The products of the official collection: one RPC request. */
export async function fetchProducts() {
  const args = Buffer.from(JSON.stringify({ from_index: 0, limit: 100 })).toString("base64");
  const res = await fetch(RPC, {
    method: "POST",
    headers: { "Content-Type": "application/json", "User-Agent": "builddeed-site-build" },
    body: JSON.stringify({
      jsonrpc: "2.0",
      id: "builddeed-build",
      method: "query",
      params: { request_type: "call_function", finality: "final", account_id: CONTRACT, method_name: "get_products", args_base64: args },
    }),
  });
  if (!res.ok) throw new Error(`get_products: HTTP ${res.status} from ${RPC}`);
  const j = await res.json();
  if (j.error || j.result?.error) throw new Error(`get_products: ${JSON.stringify(j.error || j.result.error)}`);
  const list = JSON.parse(Buffer.from(j.result.result).toString("utf8"));
  // A key that is not a valid product key never becomes a path.
  return list.filter((p) => isProductKey(p.key)).map((p) => withExtras(p));
}

const esc = (s) =>
  String(s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);

/** JSON for a <script> element: no "</script>" can end it early. */
const jsonForScript = (v) => JSON.stringify(v).replace(/</g, "\\u003c");

/** The <head> of a page. */
export function head({ title, description, path, image = "/og.png", extra = "" }) {
  return `<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>${esc(title)}</title>
  <meta name="description" content="${esc(description)}">
  <link rel="canonical" href="${SITE}${path}">
  <meta property="og:type" content="website">
  <meta property="og:title" content="${esc(title)}">
  <meta property="og:description" content="${esc(description)}">
  <meta property="og:image" content="${SITE}${image}">
  <meta property="og:url" content="${SITE}${path}">
  <meta name="twitter:card" content="summary_large_image">
  <link rel="icon" href="/favicon.svg" type="image/svg+xml">
  <link rel="alternate" type="text/markdown" href="/agents.md" title="BuildDeed for agents">${extra}
</head>`;
}

/** The static product page. */
export function productHtml(p) {
  const path = productPath(p.key);
  const art = nftImage(p.key);
  const lead = p.description || p.text || "";
  const price = formatNear(p.price);
  const ld = {
    "@context": "https://schema.org",
    "@type": "SoftwareApplication",
    name: p.name,
    description: lead,
    url: `${SITE}${path}`,
    image: p.shot ? `${SITE}${p.shot}` : art ? `${SITE}${art}` : undefined,
    operatingSystem: (p.platforms || []).join(", ") || undefined,
    offers: { "@type": "Offer", price, priceCurrency: "NEAR", description: `A license on NEAR testnet: updates for ${p.update_period_days} days, ${p.max_devices} computers, resellable.` },
  };
  return `<!doctype html>
<html lang="en">
${head({
  title: `${p.name} - BuildDeed`,
  description: `${lead} License: ${price} NEAR on NEAR testnet.`.trim(),
  path,
  image: p.shot || art,
  extra: `\n  <script type="application/ld+json">${jsonForScript(ld)}</script>`,
})}
<body>
<main id="main" class="page product-page" data-product="${esc(p.key)}">
  <article class="product-hero">
    <div class="product-media">
      ${p.shot ? `<img class="shot${p.pixel ? " pixel" : ""}" src="${esc(p.shot)}" alt="${esc(p.alt || p.name)}" width="640" height="480">` : ""}
      ${art ? `<img class="license-badge" src="${esc(art)}" alt="${esc(p.name)} license image" width="96" height="96">` : ""}
    </div>
    <div class="product-info">
      <h1>${esc(p.name)}</h1>
      <p class="lead">${esc(lead)}</p>
      <p class="price"><strong>${esc(price)} NEAR</strong> <span class="muted">for a license, on NEAR testnet</span></p>
      <div class="tags">${(p.platforms || []).map((x) => `<span class="pill">${esc(x)}</span>`).join("")}</div>
    </div>
  </article>
  <section>
    <h2>What you get</h2>
    <ul class="terms">${termLines(p).map((t) => `<li>${esc(t)}</li>`).join("")}</ul>
  </section>
  <section>
    <h2>About</h2>
    ${p.text ? `<p>${esc(p.text)}</p>` : ""}
    <dl class="kv">
      <dt>Developer</dt><dd>${esc(p.developer)}</dd>
      ${p.repo ? `<dt>Source</dt><dd><a href="${esc(p.repo)}">${esc(p.repo)}</a></dd>` : ""}
      <dt>Product key</dt><dd class="mono">${esc(p.key)}</dd>
      <dt>Contract</dt><dd class="mono">${esc(CONTRACT)}</dd>
    </dl>
  </section>
  <noscript><p>The Buy button, the downloads and your licenses need JavaScript and a NEAR wallet.</p></noscript>
</main>
<script type="module" src="/src/pages/product.js"></script>
</body>
</html>
`;
}

/** Writes p/<key>/index.html for each product under `root` (it removes old
 * pages first) and returns the Vite inputs. */
export function writeProductPages(products, root) {
  const dir = join(root, "p");
  rmSync(dir, { recursive: true, force: true });
  const inputs = {};
  for (const p of products) {
    const file = join(dir, p.key, "index.html");
    mkdirSync(join(dir, p.key), { recursive: true });
    writeFileSync(file, productHtml(p));
    inputs[`p-${p.key}`] = file;
  }
  return inputs;
}

/** One card of the app grid, as static HTML. The home page script builds
 * the same cards from live data. */
export function appCardHtml(p) {
  const media = p.shot
    ? `<span class="app-media${p.pixel ? " pixel" : ""}"><img src="${esc(p.shot)}" alt="${esc(p.alt || p.name)}" width="640" height="480" loading="lazy"></span>`
    : `<span class="app-media license"><img src="${esc(nftImage(p.key))}" alt="${esc(p.name)} license image" width="512" height="512" loading="lazy"></span>`;
  return `<a class="app-card" href="${esc(productPath(p.key))}">
      ${media}
      <span class="app-body">
        <span class="app-name">${esc(p.name)}</span>
        <span class="app-desc">${esc(p.description || "")}</span>
        <span class="app-meta"><span class="app-price">${esc(formatNear(p.price))} NEAR</span>${(p.platforms || []).length ? `<span class="muted">${esc(p.platforms.join(" · "))}</span>` : ""}</span>
        <span class="btn small app-get">Get</span>
      </span>
    </a>`;
}

/** The sitemap: the store pages, each product page, and the agent files. */
export function sitemapXml(products) {
  const paths = [
    "/",
    ...products.map((p) => productPath(p.key)),
    "/market/",
    "/verify/",
    "/developers/",
    "/developers/add/",
    "/developers/docs/",
    "/agents.md",
    "/llms.txt",
  ];
  return `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
${paths.map((p) => `  <url><loc>${SITE}${p}</loc></url>`).join("\n")}
</urlset>
`;
}

/** The license images that the build adds: nft/<key>.png for each product
 * that has no own image in public/nft/, as a copy of public/nft/default.png.
 * Returns [fileName, bytes] pairs. */
export function defaultLicenseImages(products, root) {
  const nft = join(root, "public", "nft");
  const generic = readFileSync(join(nft, "default.png"));
  return products.filter((p) => !existsSync(join(nft, `${p.key}.png`))).map((p) => [`nft/${p.key}.png`, generic]);
}
