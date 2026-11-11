// NEAR access for the BuildDeed site: contract views through RPC, and
// wallet calls through NEAR Connect.
import { NearConnector } from "@hot-labs/near-connect";

export const NETWORK = "testnet";
/** The official BuildDeed collection. Each license of each product is a
 * token of this one contract. */
export const CONTRACT = "licenses.builddeed.testnet";
/** The product of a page without ?product=. */
export const DEFAULT_PRODUCT = "checksum";
/** Old product keys of links that are still in use. */
const ALIASES = { demo: "checksum" };

/** Display data that the contract does not keep, by product key. */
const EXTRAS = {
  checksum: {
    shot: "img/checksum-nag.webp",
    alt: "BuildDeed Checksum showing its UNREGISTERED COPY screen",
    tags: ["Rust", "egui", "Linux · Windows · macOS"],
    text: "A file checksum tool. Drop a file to see its SHA-256 and its official build record. Registered: the folder check, which reports what changed since the last check.",
    pixel: false,
  },
  doom: {
    shot: "img/doom-play.webp",
    alt: "Room4Doom with Freedoom: the shotgun, health 85 and a defeated monster in E1M1",
    tags: ["Rust", "GPL-2.0-or-later", "Freedoom inside"],
    text: "It runs Doom. A Doom engine in Rust with monsters, weapons and sound. Unregistered: E1M1. Press R in the game to register this computer; a license unlocks every episode and map.",
    pixel: true,
  },
};

/** The repository of BuildDeed itself. */
export const BUILDDEED_REPO = "https://github.com/13rac1/builddeed";
/** The repository of Room4Doom, for the footer. */
export const ROOM4DOOM_REPO = "https://github.com/ai-13rac1/room4doom";
export const RPC = "https://rpc.testnet.fastnear.com";
export const EXPLORER = "https://testnet.nearblocks.io";
export const FAUCET = "https://near-faucet.io/";
const GAS = "100000000000000"; // 100 Tgas

const YOCTO = 10n ** 24n;

/** A product key from a link, with the old keys changed to the new ones. */
export function productKey(key) {
  if (!key) return null;
  return ALIASES[key] || key;
}

/** The product key of this page: ?product=doom, or #product=doom (the
 * register page), or the default product. */
export const PRODUCT_KEY =
  productKey(new URLSearchParams(location.search).get("product")) ||
  productKey(new URLSearchParams(location.hash.slice(1)).get("product")) ||
  DEFAULT_PRODUCT;

/** A link to another page of this site for a product (a key or a product). */
export function pageLink(page, product = PRODUCT_KEY) {
  const key = typeof product === "string" ? product : product.key;
  return key === DEFAULT_PRODUCT ? page : `${page}?product=${encodeURIComponent(key)}`;
}

/** A product view of the contract with the fields that the site adds. */
function withExtras(view, contract) {
  return { ...view, contract, repo: view.url || null, ...(EXTRAS[view.key] || {}) };
}

let productsPromise = null;

/** The products of the official collection, in the order of the contract.
 * The page reads them once. */
export function loadProducts() {
  if (!productsPromise) {
    productsPromise = viewAt(CONTRACT, "get_products", { from_index: 0, limit: 100 })
      .then((list) => list.map((p) => withExtras(p, CONTRACT)))
      .catch((e) => {
        productsPromise = null;
        throw e;
      });
  }
  return productsPromise;
}

/** The product with this key (default: the product of this page). It
 * throws when the collection has no such product. */
export async function loadProduct(key = PRODUCT_KEY) {
  const p = (await loadProducts()).find((x) => x.key === key);
  if (!p) throw new Error(`No product has the key "${key}"`);
  return p;
}

/** Calls a view method of the official collection. */
export function view(method, args = {}) {
  return viewAt(CONTRACT, method, args);
}

/** Calls a view method of the contract of `product`. */
export function viewOf(product, method, args = {}) {
  return viewAt(product.contract, method, args);
}

async function viewAt(contract, method, args) {
  const res = await fetch(RPC, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      jsonrpc: "2.0",
      id: "builddeed",
      method: "query",
      params: {
        request_type: "call_function",
        finality: "optimistic",
        account_id: contract,
        method_name: method,
        args_base64: btoa(JSON.stringify(args)),
      },
    }),
  });
  const j = await res.json();
  if (j.error) throw new Error(j.error.data || j.error.message || JSON.stringify(j.error));
  if (j.result.error) throw new Error(j.result.error);
  return JSON.parse(new TextDecoder().decode(new Uint8Array(j.result.result)));
}

let connector;
let account = null;
const listeners = new Set();

function setAccount(a) {
  account = a;
  listeners.forEach((f) => f(account));
}

/** The NEAR Connect instance. It loads the wallet list on first use. */
export async function getConnector() {
  if (!connector) {
    connector = new NearConnector({ network: NETWORK, features: { testnet: true } });
    connector.on("wallet:signIn", (t) => setAccount(t.accounts[0]?.accountId ?? null));
    connector.on("wallet:signOut", () => setAccount(null));
    await connector.whenManifestLoaded;
    try {
      const wallet = await connector.wallet();
      const accounts = await wallet.getAccounts();
      setAccount(accounts[0]?.accountId ?? null);
    } catch {
      setAccount(null);
    }
  } else {
    await connector.whenManifestLoaded;
  }
  return connector;
}

/** Calls `f(accountId or null)` now and at each sign-in and sign-out. */
export function onAccount(f) {
  listeners.add(f);
  f(account);
}

export function currentAccount() {
  return account;
}

export async function signIn() {
  const c = await getConnector();
  await c.connect();
}

export async function signOut() {
  const c = await getConnector();
  await c.disconnect();
  setAccount(null);
}

/** Sends one function call to a license contract (default: the official
 * collection). */
export async function call(methodName, args, depositYocto = "0", contract = CONTRACT) {
  const c = await getConnector();
  if (!account) await c.connect();
  const wallet = await c.wallet();
  const outcome = await wallet.signAndSendTransaction({
    receiverId: contract,
    actions: [{ type: "FunctionCall", params: { methodName, args, gas: GAS, deposit: depositYocto } }],
  });
  // Some wallets return a failed transaction instead of an error.
  const failure = outcome?.status?.Failure;
  if (failure) {
    const kind = failure.ActionError?.kind;
    throw new Error(kind?.FunctionCallError?.ExecutionError || JSON.stringify(failure));
  }
  return outcome;
}

/** The error text of a failed contract call: the contract message when the
 * text has one, else the full text. */
export function contractError(e) {
  const msg = e?.message || String(e);
  const m = msg.match(/Smart contract panicked: ([^"\\}]+)/);
  return m ? m[1].trim() : msg;
}

/** yoctoNEAR string -> "5" or "2.61" (at most 4 decimals). */
export function formatNear(yocto) {
  const y = BigInt(yocto);
  const whole = y / YOCTO;
  const frac = ((y % YOCTO) * 10000n) / YOCTO;
  if (frac === 0n) return whole.toString();
  return `${whole}.${frac.toString().padStart(4, "0").replace(/0+$/, "")}`;
}

/** "2.5" -> yoctoNEAR string. */
export function parseNear(text) {
  const m = String(text).trim().match(/^(\d+)(?:\.(\d{1,24}))?$/);
  if (!m) throw new Error("Give an amount such as 3 or 2.5");
  return (BigInt(m[1]) * YOCTO + BigInt((m[2] || "").padEnd(24, "0"))).toString();
}

/** "doom:7" -> "7": the number of a license within its product. */
export function tokenNumber(tokenId) {
  return String(tokenId).split(":").pop();
}

export function formatDate(ms) {
  return new Date(Number(ms)).toISOString().slice(0, 10);
}

export function accountLink(id) {
  return `${EXPLORER}/address/${encodeURIComponent(id)}`;
}
