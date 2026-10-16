// Reconcile only verified native class bindings; preserve every existing annotation.
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { execFileSync } from "node:child_process";
import assert from "node:assert/strict";
import { validateBridgeProof, validateProofLogName } from "./bridge-proof.mjs";
export { validateBridgeProof } from "./bridge-proof.mjs";
import { assertQualifiedTarget, splitQualifiedTarget } from "./bridge-targets.mjs";

export async function main() {
const root = process.cwd();
assert(
  process.argv.includes("--write"),
  "explicit --write required after proof",
);
const sha = execFileSync("git", ["rev-parse", "HEAD"], {
  encoding: "utf8",
}).trim();
const receipts = JSON.parse(
  fs.readFileSync(
    path.join(
      root,
      "sdk-rs/safe-cex-venue-cryptocom/evidence",
      "bridge-controls-" + sha.slice(0, 10),
      "controls.json",
    ),
    "utf8",
  ),
);
const evidenceDirectory = path.join(root, "sdk-rs/safe-cex-venue-cryptocom/evidence", "bridge-controls-" + sha.slice(0, 10));
const logs = new Map();
for (const record of receipts) {
  const name = validateProofLogName(record.log);
  const filename = path.join(evidenceDirectory, name);
  assert(fs.lstatSync(filename).isFile(), "proof log must be a regular file");
  assert.equal(path.dirname(fs.realpathSync(filename)), fs.realpathSync(evidenceDirectory), "proof log escapes evidence directory");
  logs.set(name, fs.readFileSync(filename, "utf8"));
}
validateBridgeProof(receipts, { sha, logs });
const { parseManifest, serializeManifest } = await import(
  pathToFileURL(path.join(root, "sdk-rs/parity/extract-ts/src/manifest.ts"))
    .href
);
const prefix = "safe-cex-venue-cryptocom/src/";
const target = (module, name) => prefix + module + ".rs::" + name;
const exchanges = {
  dispose: ["class_admin", "dispose"],
  getAccount: ["class_admin", "get_account"],
  validateAccount: ["class_admin", "validate_account"],
  setLeverage: ["class_admin", "admin_post"],
  cancelAllOrders: ["class_admin", "admin_post"],
  changePositionMode: ["class_admin", "change_position_mode"],
  changeMarginType: ["class_admin", "change_margin_type"],
};
for (const name of [
  "mapOrderStatus",
  "mapOrderType",
  "mapCryptocomOrderType",
  "mapTimeInForce",
  "mapStopTriggerType",
  "mapCryptocomTimeInForce",
])
  exchanges[name] = ["class_mappings", "member"];
const sockets = {
  connectAndSubscribe: "connect",
  onOpen: "on_open",
  shouldEnforcePongMissReconnect: "member",
  getWatchdogStaleThresholdMs: "member",
  getWatchdogPingThresholdMs: "member",
  ping: "member",
  resubscribe: "resubscribe",
  sendSubscriptionFrames: "send_frames",
  registerAndSubscribe: "register",
  subscribeToOrderBook: "symbol_channel",
  subscribeToTicker: "symbol_channel",
  subscribeToTickers: "member",
  subscribeToMarketData: "market_data",
  subscribeToTrades: "symbol_channel",
  subscribeToCandlestick: "symbol_channel",
  handleValuationUpdate: "valuation",
  getMarketIdsByIndexInstrument: "index_markets",
  handleCandlestickUpdate: "candle",
  authenticate: "authenticate",
  subscribeToOrderUpdates: "order_updates",
  subscribeToPositionUpdates: "subscribe_one",
  subscribeToBalanceUpdates: "subscribe_one",
  unsubscribe: "unsubscribe",
  dispose: "dispose",
};
const anchors = [
  "shared-utf8-string-boundary",
  "closed-typed-output-boundary",
  "owned-snapshot-vs-live-alias",
  "numeric-ingress-defect",
  "divergence-taxonomy",
  "forced-surrogate-conflict",
]
  .map((x) => "sdk-rs/parity/MANIFEST-SPEC.md#" + x)
  .join("; ");
let closed = 0;
for (const [file, klass, side] of [
  ["cryptocom.exchange.ts", "CryptocomExchange", "exchange"],
  ["cryptocom.ws-public.ts", "CryptocomPublicWebSocket", "public"],
  ["cryptocom.ws-private.ts", "CryptocomPrivateWebSocket", "private"],
]) {
  const filename = path.join(
    root,
    "sdk-rs/parity/manifest/exchanges/cryptocom",
    file + ".toml",
  );
  const manifest = parseManifest(
    fs.readFileSync(filename, "utf8"),
    "packages/safe-cex/src/exchanges/cryptocom/" + file,
    filename,
  );
  for (const row of manifest.symbol) {
    const additions = [];
    if (row.ts === klass) {
      assert.equal(row.status, "divergent");
      assert(row.note.startsWith("FINDING:"));
      row.status = "ported";
      row.layer = "adapted";
      closed++;
      additions.push(
        target(
          side === "exchange" ? "class_exchange" : "class_socket",
          "member",
        ),
        target("native_bindings", "BindingOps"),
        target("native_bindings", "BoundHost"),
        target("native_bindings", "Prepared"),
        target("native_bindings", "Awaited"),
        ...["get", "prepare", "call", "global", "wait", "method", "math", "each"].map(name => target("native_bindings", "BoundHost::" + name)),
      );
      row.note =
        "Owning class assembles every own application field directly from Rust bodies, with concrete ReadHost/WriteHost bindings and native administrative/socket/mapping bodies. No existing_member application provider remains. src/bridge_tests.rs: native identity/error/receiver/await-turn/observer-drop/retained-callback/endpoint controls; source entry floor65 stops native index64 private ping and source index64 private dispose. Entry smoke is separate from body tests and scheduling controls; no exhaustive input/host/engine claim. Measured exits and discrimination receipts: GATE_EVIDENCE.md. Canonical boundaries: " +
        anchors +
        ".";
    } else if (row.ts === klass + ".constructor") {
      additions.push(
        target(
          side === "exchange" ? "class_exchange" : "class_socket",
          "member",
        ),
      );
      row.note = row.note
        .replace(
          "Existing typed arrow factories remain a declared provider dependency and the owning class carries their bridge FINDING.",
          "Own application fields bind concrete Rust bodies through the native owning-class factory.",
        )
        .replace(
          "Class-level existing-member assembly FINDING remains separate.",
          "Owning-class application field bindings are concrete Rust factories.",
        );
    } else if ((row.ts.startsWith(klass + ".") || row.ts.startsWith(klass + "#"))) {
      const name = row.ts.slice(klass.length + 1);
      if (side === "exchange" && exchanges[name])
        additions.push(
          target(...exchanges[name]),
          target("class_exchange", "member"),
        );
      if (side === "exchange" && ["fetchMarkets", "fetchTickers", "fetchOHLCV", "fetchOrderBook", "fetchOrders", "fetchPositions", "fetchBalance", "mapOrder", "placeOrder", "cancelOrder", "cancelOrders", "cancelSymbolOrders", "updateOrder", "placeOrders", "listenOHLCV", "listenOrderBook"].includes(name)) additions.push(
        target("class_exchange", "member"), target("native_bindings", "BoundHost"),
      );
      if (side !== "exchange" && sockets[name])
        additions.push(
          target("class_socket", sockets[name]),
          target("class_socket", "member"),
        );
      if (
        side === "private" &&
        ["mapOrderType", "mapOrderStatus", "mapTimeInForce"].includes(name)
      )
        additions.push(
          target("class_mappings", "member"),
          target("class_socket", "member"),
        );
    }
    if (row.ts === "CryptocomExchange.fetchMarkets") additions.push(
      target("rest_reads", "ReadHost::imported"),
      target("rest_reads", "ReadHost::prepare_parse_float"),
      target("rest_reads", "ReadHost::call_parse_float"),
    );
    if (!additions.length) continue;
    const existing = Array.isArray(row.rs) ? row.rs : row.rs ? [row.rs] : [];
    const extra = [...new Set(additions)].filter((x) => !existing.includes(x));
    for (const value of extra) {
      const [relative, name] = splitQualifiedTarget(value);
      const source = fs.readFileSync(
        path.join(root, "sdk-rs", relative),
        "utf8",
      );
      assertQualifiedTarget(source, name, value);
    }
    row.rs = [...existing, ...extra];
    row.layer = "adapted";
    if (row.ts === klass)
      row.note +=
        " Adapted targets: " +
        row.rs.map((x) => "`" + x + "`").join(", ") +
        ".";
    else if (extra.length)
      row.note +=
        " Native owning-class split targets: " +
        extra.map((x) => "`" + x + "`").join(", ") +
        ".";
  }
  manifest.file.rust_files = [
    ...new Set([
      ...(manifest.file.rust_files || []),
      ...manifest.symbol
        .flatMap((row) => (typeof row.rs === "string" ? [row.rs] : row.rs))
        .filter(Boolean)
        .map((x) => x.split("::")[0]),
    ]),
  ];
  fs.writeFileSync(filename, serializeManifest(manifest));
}
assert.equal(
  closed,
  3,
  "three owning-class findings, including exchange introduced by initial five-row slice",
);
console.log(
  "Closed3 divergent class rows; existing annotations preserved. Run parity:extract writer, then check.",
);

}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  await main();
}
