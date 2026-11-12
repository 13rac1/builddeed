// Product data and number formats that the browser pages and the build
// (vite.config.js) both use. No browser API here: Node imports this file.

/** The address of the website. Absolute URLs (OG tags, sitemap) use it. */
export const SITE = "https://builddeed.dev";
export const NETWORK = "testnet";
/** The official BuildDeed collection. Each license of each product is a
 * token of this one contract. */
export const CONTRACT = "licenses.builddeed.testnet";
export const RPC = "https://rpc.testnet.fastnear.com";
/** The product of an old link without a product. */
export const DEFAULT_PRODUCT = "checksum";

/** Display data that the contract does not keep, by product key. */
export const EXTRAS = {
  checksum: {
    shot: "/img/checksum-nag.webp",
    alt: "BuildDeed Checksum showing its UNREGISTERED COPY screen",
    tags: ["Rust", "Slint"],
    platforms: ["Linux", "Windows", "macOS"],
    text: "A file checksum tool. Drop a file to see its SHA-256 and its official build record. Registered: the folder check, which reports what changed since the last check.",
    pixel: false,
  },
  doom: {
    shot: "/img/doom-play.webp",
    alt: "Room4Doom with Freedoom: the shotgun, health 85 and a defeated monster in E1M1",
    tags: ["Rust", "GPL-2.0-or-later", "Freedoom inside"],
    platforms: ["Linux", "macOS"],
    text: "It runs Doom. A Doom engine in Rust with monsters, weapons and sound. Unregistered: E1M1. Press R in the game to register this computer; a license unlocks every episode and map.",
    pixel: true,
  },
};

/** Product keys that have a license image in public/nft/. */
export const NFT_IMAGES = ["checksum", "doom"];

/** The path of the product page. */
export function productPath(key) {
  return `/p/${encodeURIComponent(key)}/`;
}

/** The license image of a product, or null. */
export function nftImage(key) {
  return NFT_IMAGES.includes(key) ? `/nft/${key}.svg` : null;
}

/** 2 to 32 characters a-z, 0-9 and -, starting with a letter or digit. */
export function isProductKey(k) {
  return typeof k === "string" && k.length >= 2 && k.length <= 32 && /^[a-z0-9][a-z0-9-]*$/.test(k);
}

/** A product view of the contract with the fields that the site adds. */
export function withExtras(view, contract = CONTRACT) {
  return { ...view, contract, repo: view.url || null, ...(EXTRAS[view.key] || {}) };
}

const YOCTO = 10n ** 24n;

/** yoctoNEAR string -> "5" or "2.61" (at most 4 decimals). */
export function formatNear(yocto) {
  const y = BigInt(yocto);
  const whole = y / YOCTO;
  const frac = ((y % YOCTO) * 10000n) / YOCTO;
  if (frac === 0n) return whole.toString();
  return `${whole}.${frac.toString().padStart(4, "0").replace(/0+$/, "")}`;
}

/** The terms of a product in plain words, one item for each term. */
export function termLines(p) {
  return [
    `Updates for ${p.update_period_days} days: you keep each official build released in that time.`,
    `Use it on ${p.max_devices} computer${p.max_devices === 1 ? "" : "s"}. Move a seat at any time.`,
    `Resell it at any time on the used license market, for ${formatNear(p.renewal_price)} NEAR or more.`,
    `${p.royalty_bps / 100}% of each resale goes to the developers and contributors.`,
    `Renew for ${formatNear(p.renewal_price)} NEAR to add ${p.update_period_days} days of updates.`,
  ];
}
