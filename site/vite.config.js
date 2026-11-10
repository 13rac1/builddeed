import { resolve } from "node:path";
import { defineConfig } from "vite";

const pages = ["index", "buy", "licenses", "market", "register", "verify"];

export default defineConfig({
  // Relative paths: the site works at 13rac1.github.io/builddeed/ and at a
  // custom domain without a rebuild.
  base: "./",
  build: {
    outDir: "dist",
    rollupOptions: {
      input: Object.fromEntries(pages.map((p) => [p, resolve(import.meta.dirname, `${p}.html`)])),
    },
  },
});
