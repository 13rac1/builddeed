// The app opens this page as sign.html#nonce=..&port=..&message=..&recipient=..
// The wallet signs the message (NEP-413). The page then sends the result to
// the app on http://127.0.0.1:<port>/callback. The app checks everything
// again; this page only carries the data.
import { h, mountChrome, busy, showError } from "../ui.js";
import { getConnector, currentAccount } from "../near.js";

mountChrome();
const main = document.getElementById("main");

const params = new URLSearchParams(location.hash.slice(1));
const nonceB64 = params.get("nonce") || "";
const port = Number(params.get("port"));
const message = params.get("message") || "";
const recipient = params.get("recipient") || "";

function b64urlToBytes(s) {
  const b64 = s.replace(/-/g, "+").replace(/_/g, "/") + "===".slice((s.length + 3) % 4);
  return Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
}

function problem() {
  if (!Number.isInteger(port) || port < 1024 || port > 65535) return "The port is not valid.";
  if (recipient !== "builddeed") return "The recipient must be builddeed.";
  if (!/^Register [ -~]{1,64}$/.test(message)) return "The message is not a BuildDeed registration.";
  try {
    if (b64urlToBytes(nonceB64).length !== 32) return "The nonce must be 32 bytes.";
  } catch {
    return "The nonce is not valid.";
  }
  return null;
}

function sendToApp(signed) {
  const q = new URLSearchParams({
    nonce: nonceB64,
    accountId: signed.accountId,
    publicKey: signed.publicKey,
    signature: signed.signature,
  });
  location.href = `http://127.0.0.1:${port}/callback?${q}`;
}

/** Signs with the connected wallet, or signs in and signs in one step. */
async function sign() {
  const nonce = b64urlToBytes(nonceB64);
  const connector = await getConnector();
  if (currentAccount()) {
    const wallet = await connector.wallet();
    return wallet.signMessage({ message, recipient, nonce });
  }
  const done = new Promise((resolve, reject) => {
    connector.on("wallet:signInAndSignMessage", (t) => {
      const s = t.accounts[0]?.signedMessage;
      s ? resolve({ ...s, accountId: s.accountId || t.accounts[0].accountId }) : reject(new Error("The wallet did not sign"));
    });
  });
  await connector.connect({ signMessageParams: { message, recipient, nonce } });
  return done;
}

const err = problem();
if (err) {
  main.replaceChildren(
    h("h1", {}, "Register the app"),
    h("p", { class: "warn-text" }, `This link is not valid: ${err}`),
    h("p", {}, "Open this page from the app: click \"I have a license\" in BuildDeed Checksum.")
  );
} else {
  const button = h("button", { class: "btn" }, "Sign with my wallet");
  button.addEventListener("click", () =>
    busy(button, async () => {
      const signed = await sign();
      if (!signed?.signature) throw new Error("The wallet did not return a signature");
      main.append(h("p", { class: "ok-text" }, "Signed. Sending the result to the app..."));
      sendToApp(signed);
    })
  );
  main.replaceChildren(
    h("h1", {}, "Register the app"),
    h("p", { class: "lead" }, "BuildDeed Checksum on your computer asks your wallet to sign this message:"),
    h("div", { class: "card" }, h("p", { class: "mono" }, message), h("p", { class: "muted" }, `Recipient: ${recipient}. This is not a transaction: it costs nothing and moves nothing.`)),
    button,
    h(
      "p",
      { class: "muted" },
      `The page then sends the signature to the app on this computer (127.0.0.1, port ${port}). The app checks the signature and asks the NEAR contract whether your account owns a license.`
    )
  );
}

getConnector().catch(showError);
