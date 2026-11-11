// NEAR access for the BuildDeed site: contract views through RPC, and
// wallet calls through NEAR Connect.
import { NearConnector } from "@hot-labs/near-connect";

export const NETWORK = "testnet";
/** The products that this site sells. Each has its own license contract. */
export const PRODUCTS = {
  demo: {
    key: "demo",
    name: "BuildDeed Checksum",
    contract: "demo.builddeed.testnet",
    repo: "https://github.com/13rac1/builddeed",
  },
  doom: {
    key: "doom",
    name: "Iron Doom",
    contract: "doom.builddeed.testnet",
    repo: "https://github.com/ai-13rac1/iron-doom",
  },
};

/** The product of this page: ?product=doom, or #product=doom (the register
 * page), or the demo. */
function pageProduct() {
  const fromQuery = new URLSearchParams(location.search).get("product");
  const fromHash = new URLSearchParams(location.hash.slice(1)).get("product");
  return PRODUCTS[fromQuery] || PRODUCTS[fromHash] || PRODUCTS.demo;
}

export const PRODUCT = pageProduct();
export const CONTRACT = PRODUCT.contract;

/** A link to another page of this site for the same product. */
export function pageLink(page, product = PRODUCT) {
  return product.key === "demo" ? page : `${page}?product=${product.key}`;
}
export const RPC = "https://rpc.testnet.fastnear.com";
export const EXPLORER = "https://testnet.nearblocks.io";
/** The source repository of the product of this page. */
export const REPO = PRODUCT.repo;
/** The repository of BuildDeed itself. */
export const BUILDDEED_REPO = "https://github.com/13rac1/builddeed";
export const FAUCET = "https://near-faucet.io/";
const GAS = "100000000000000"; // 100 Tgas

const YOCTO = 10n ** 24n;

/** Calls a view method of the license contract of this page. */
export function view(method, args = {}) {
  return viewOf(PRODUCT, method, args);
}

/** Calls a view method of the license contract of `product`. */
export async function viewOf(product, method, args = {}) {
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
        account_id: product.contract,
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

/** Sends one function call to the license contract. */
export async function call(methodName, args, depositYocto = "0") {
  const c = await getConnector();
  if (!account) await c.connect();
  const wallet = await c.wallet();
  return wallet.signAndSendTransaction({
    receiverId: CONTRACT,
    actions: [{ type: "FunctionCall", params: { methodName, args, gas: GAS, deposit: depositYocto } }],
  });
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

export function formatDate(ms) {
  return new Date(Number(ms)).toISOString().slice(0, 10);
}

export function accountLink(id) {
  return `${EXPLORER}/address/${encodeURIComponent(id)}`;
}
