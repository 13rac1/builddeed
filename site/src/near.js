// NEAR access for the BuildDeed site: contract views through RPC, and
// wallet calls through NEAR Connect.
import { NearConnector } from "@hot-labs/near-connect";

import { CONTRACT, NETWORK, RPC, DEFAULT_PRODUCT, withExtras, formatNear, productPath } from "./catalog.js";

export { CONTRACT, NETWORK, RPC, DEFAULT_PRODUCT, formatNear, productPath };
export { SITE, EXTRAS, nftImage, termLines, isProductKey } from "./catalog.js";

/** The repository of BuildDeed itself. */
export const BUILDDEED_REPO = "https://github.com/13rac1/builddeed";
/** The repository of the BuildDeed SDK (Rust and Go). */
export const SDK_REPO = "https://github.com/13rac1/builddeed-sdk";
/** The repository of Room4Doom, for the footer. */
export const ROOM4DOOM_REPO = "https://github.com/ai-13rac1/room4doom";
export const EXPLORER = "https://testnet.nearblocks.io";
export const FAUCET = "https://near-faucet.io/";
const GAS = "100000000000000"; // 100 Tgas

const YOCTO = 10n ** 24n;

/** The product key of a link, or null. */
export function productKey(key) {
  return key || null;
}

/** The product key in ?product=, or null. */
export function queryProduct() {
  return productKey(new URLSearchParams(location.search).get("product"));
}

/** The product key in #product= (the register page), or the default. */
export function hashProduct() {
  return productKey(new URLSearchParams(location.hash.slice(1)).get("product")) || DEFAULT_PRODUCT;
}

/** The product key in a /p/<key>/ path, or null. */
export function pathProduct(pathname = location.pathname) {
  const m = pathname.match(/^\/p\/([^/]+)\/?(?:index\.html)?$/);
  return m ? productKey(decodeURIComponent(m[1])) : null;
}

/** The used license market, for one product or for all. */
export function marketLink(key) {
  return key ? `/market/?product=${encodeURIComponent(key)}` : "/market/";
}

/** The verify page of a product. */
export function verifyLink(key) {
  return `/verify/?product=${encodeURIComponent(key)}`;
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

/** The product with this key. It throws when the collection has no such
 * product. */
export async function loadProduct(key) {
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
