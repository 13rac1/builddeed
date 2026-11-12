import { resolve } from "node:path";
import { defineConfig } from "vite";
import { fetchProducts, writeProductPages, appCardHtml, sitemapXml, defaultLicenseImages } from "./scripts/static-pages.mjs";

const root = import.meta.dirname;

// The pages that are files in the source tree: each page is a folder with
// an index.html. 404.html is the page for a missing path.
const pages = [
  "index.html",
  "404.html",
  "market/index.html",
  "licenses/index.html",
  "verify/index.html",
  "register/index.html",
  "developers/index.html",
  "developers/add/index.html",
  "developers/docs/index.html",
];

// One get_products request at build time: a static page for each product
// (p/<key>/index.html, not in git), the app grid of the home page, the
// sitemap, and nft/<key>.png for each product with no own license image.
// The build fails when the RPC does not answer.
const products = await fetchProducts();
const productInputs = writeProductPages(products, root);

export default defineConfig({
  // Absolute paths: 404.html is served at any missing path, so its assets
  // and links cannot be relative. The site is at the root of builddeed.dev.
  base: "/",
  plugins: [
    {
      name: "builddeed-static-data",
      transformIndexHtml: {
        order: "pre",
        handler(html) {
          return html.replace("<!--app-grid-->", products.map(appCardHtml).join("\n    "));
        },
      },
      generateBundle() {
        this.emitFile({ type: "asset", fileName: "sitemap.xml", source: sitemapXml(products) });
        for (const [fileName, source] of defaultLicenseImages(products, root)) {
          this.emitFile({ type: "asset", fileName, source });
        }
      },
    },
  ],
  build: {
    outDir: "dist",
    rollupOptions: {
      input: {
        ...Object.fromEntries(pages.map((p) => [p.replace(/[^a-z0-9]+/g, "-"), resolve(root, p)])),
        ...productInputs,
      },
    },
  },
});
