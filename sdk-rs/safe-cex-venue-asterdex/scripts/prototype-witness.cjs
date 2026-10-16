// Maintained live-source witnesses for pending prototype-sensitive socket rows.
const assert = require("node:assert/strict"),
  fs = require("node:fs"),
  path = require("node:path");
const { ts, root } = require("./source.cjs"),
  printer = ts.createPrinter();
const negative = process.argv.includes("--negative-control");
const shared = ts.createSourceFile(
  "types.ts",
  fs.readFileSync(path.join(root, "packages/safe-cex/src/types.ts"), "utf8"),
  ts.ScriptTarget.Latest,
  true,
);
const logging = fs.readFileSync(
  path.join(root, "packages/logging/src/LogSeverity.ts"),
  "utf8",
);
const enums =
  logging +
  "\n" +
  shared.statements
    .filter(ts.isEnumDeclaration)
    .map((n) => printer.printNode(ts.EmitHint.Unspecified, n, shared))
    .join("\n");
const parserSource = fs.readFileSync(
  path.join(root, "packages/json-parse/src/jsonParse.ts"),
  "utf8",
);
const parser = {};
new Function(
  "exports",
  ts.transpileModule(parserSource, {
    compilerOptions: {
      target: ts.ScriptTarget.ES2022,
      module: ts.ModuleKind.CommonJS,
    },
  }).outputText,
)(parser);
function load(file, className, names) {
  let text = fs.readFileSync(
    path.join(root, "packages/safe-cex/src/exchanges/asterdex", file),
    "utf8",
  );
  if (negative && file === "asterdex.ws-public.ts") {
    const before = "private topics: SubscribedTopics = {};";
    assert(text.includes(before), "topic control source missing");
    text = text.replace(
      before,
      "private topics: SubscribedTopics = Object.create(null);",
    );
  }
  const ast = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true),
    cls = ast.statements.find(ts.isClassDeclaration);
  const members = names
    .map((name) => {
      const n = cls.members.find((n) => n.name?.getText(ast) === name);
      assert(n, "missing " + name);
      let printed = printer.printNode(ts.EmitHint.Unspecified, n, ast);
      if (name === "handleStreamData") {
        // Observe the live lookup before its result is coerced into a handler key.
        const lookup = "const mappedType = eventTypeMap[eventType];";
        assert(printed.includes(lookup), "stream lookup capture source missing");
        if (process.env.ASTERDEX_PUBLIC_STREAM_CONTROL === "own-only") {
          printed = printed.replace(
            lookup,
            "const mappedType = Object.hasOwn(eventTypeMap, eventType) ? eventTypeMap[eventType] : undefined;this.lookupCapture = {eventType, mappedType};",
          );
        } else {
          printed = printed.replace(lookup, lookup + "this.lookupCapture = {eventType, mappedType};");
        }
      }
      if (
        process.env.ASTERDEX_PUBLIC_EVENT_CONTROL === "own-only" &&
        file === "asterdex.ws-public.ts" &&
        name === "handleEventData"
      ) {
        const guard = "this.messageHandlers[eventType]";
        assert(printed.includes(guard), "event-data guard source missing");
        printed = printed.replace(
          guard,
          "Object.hasOwn(this.messageHandlers, eventType) && " + guard,
        );
      }
      return printed;
    })
    .join("\n");
  const constants = ast.statements
    .filter(
      (n) =>
        ts.isVariableStatement(n) &&
        n.declarationList.declarations.some(
          (d) => d.name.getText(ast) === "PARTIAL_DEPTH_STREAM",
        ),
    )
    .map((n) => printer.printNode(ts.EmitHint.Unspecified, n, ast))
    .join("\n");
  const out = {};
  new Function(
    "exports",
    "jsonParse",
    "WebSocket",
    ts.transpileModule(
      enums +
        "\n" +
        constants +
        "\nexport class " +
        className +
        "{" +
        members +
        "}",
      {
        compilerOptions: {
          target: ts.ScriptTarget.ES2022,
          module: ts.ModuleKind.CommonJS,
        },
      },
    ).outputText,
  )(out, parser.jsonParse, { OPEN: 1 });
  return out[className];
}
const Public = load("asterdex.ws-public.ts", "AsterDEXPublicWebsocket", [
  "PING_REQUEST_ID",
  "topics",
  "subscriptionId",
  "messageHandlers",
  "onMessage",
  "handleStreamData",
  "handleEventData",
  "subscribe",
  "unsubscribe",
]);
const Private = load("asterdex.ws-private.ts", "AsterDEXPrivateWebsocket", [
  "messageHandlers",
  "onMessage",
  "handleExecutionReport",
  "handleOrderTradeUpdate",
  "getOrderUpdateTime",
  "handleListenKeyExpired",
]);
function fixture(Cls) {
  const o = new Cls(),
    trace = [];
  o.isDisposed = false;
  o.trackLastReceived = () => trace.push("received");
  o.handlePongEvent = () => trace.push("pong");
  o.recordMessageHandlerStats = (k) => trace.push(["stats", k]);
  o.parent = {
    log: (...a) => trace.push(["log", ...a]),
    store: { orders: [] },
    emitter: { emit: (...a) => trace.push(["emit", ...a]) },
  };
  o.ws = { readyState: 1 };
  o.send = (m) => trace.push(["send", JSON.parse(m), m]);
  for (const name of [
    "handleTickerUpdate",
    "handleMarkPriceUpdate",
    "handleTradeUpdate",
    "handleKlineUpdate",
    "handleDepthSnapshot",
    "handleAccountPositionUpdate",
    "handleAccountUpdate",
  ])
    o[name] = () => trace.push(name);
  o.scheduleOrderReconcile = () => trace.push("reconcile");
  o.clearListenKeyRefreshTimer = () => trace.push("clear-refresh");
  o.clearListenKeyWatchdogTimer = () => trace.push("clear-watchdog");
  o.reconnect = () => trace.push("reconnect");
  return { o, trace };
}
const keys = [
  "constructor",
  "toString",
  "valueOf",
  "hasOwnProperty",
  "isPrototypeOf",
  "propertyIsEnumerable",
  "toLocaleString",
  "__proto__",
  "__defineGetter__",
  "__defineSetter__",
  "__lookupGetter__",
  "__lookupSetter__",
];
(async () => {
  const evidence = [];
  let { o: expiredOracle, trace: expiredTrace } = fixture(Private);
  expiredOracle.handleListenKeyExpired({});
  const listenKeyExpired = expiredTrace;
  for (const key of keys) {
    const inherited =
      key === "__proto__" ? Object.prototype : Object.prototype[key];
    let { o, trace } = fixture(Public);
    o.onMessage({ data: JSON.stringify([{ e: key }, { e: "aggTrade" }]) });
    assert.deepEqual(
      trace,
      ["received", ["stats", key]],
      "prototype dispatch throws and stops the public batch",
    );
    evidence.push({ case: "public batch", key, trace });
    ({ o, trace } = fixture(Public));
    o.onMessage({ data: JSON.stringify({ stream: "btc@" + key, data: {} }) });
    assert.deepEqual(
      trace,
      ["received"],
      "inherited mapped type does not name an actual handler",
    );
    const truncated = key.startsWith("_");
    assert.equal(o.lookupCapture.eventType, truncated ? "" : key);
    assert.equal(o.lookupCapture.mappedType, truncated ? undefined : inherited,
      `handleStreamData inherited lookup preservation ${key}`);
    evidence.push({ case: "public stream", key, lookup: truncated ? "truncated-missing" : "inherited", trace });
    ({ o, trace } = fixture(Private));
    o.onMessage({ data: JSON.stringify({ e: key }) });
    assert.deepEqual(
      trace.slice(0, 2),
      ["received", ["stats", key]],
      "private prototype guard passes",
    );
    assert.equal(trace.length, 3);
    assert.equal(trace[2][0], "log");
    assert.match(
      trace[2][1],
      /^Failed to process private message: .*forEach is not a function$/,
      "private prototype throw is caught and logged",
    );
    evidence.push({ case: "private event", key, trace });
    ({ o, trace } = fixture(Public));
    await o.subscribe(["btc@" + key]);
    assert.equal(
      Object.hasOwn(o.topics, key),
      false,
      "prototype topic suppression must preserve the inherited truthy guard",
    );
    assert.equal(trace[0][1].method, "SUBSCRIBE");
    evidence.push({ case: "topic", key, own: Object.keys(o.topics), trace });
    for (const method of ["handleExecutionReport", "handleOrderTradeUpdate"]) {
      ({ o, trace } = fixture(Private));
      o.parent.store.orders = [{ id: "1" }];
      o.parent.store.updateOrder = (_, patch) =>
        trace.push(["patch", typeof patch.status, patch.status === inherited]);
      const input = { i: 1, X: key, z: "1", q: "2", T: 123 };
      o[method](method === "handleExecutionReport" ? input : { o: input });
      assert.deepEqual(
        trace.find((e) => Array.isArray(e) && e[0] === "patch"),
        ["patch", typeof inherited, true],
        "prototype order status suppresses Open fallback",
      );
      evidence.push({
        case: method,
        key,
        trace: trace.filter((e) => typeof e === "string" || e[0] === "patch"),
      });
    }
  }
  const streamDispatch = [];
  for (const [stream, data] of [
    ["btc@ticker", {}],
    ["eth@aggTrade", { s: "kept" }],
    ["xrp@markPrice", {}],
    ["ada@kline_1m", {}],
    ["btc@depth", {}],
    ["btc@depth20@100ms", {}],
    ["btc@depth20@100ms\n", {}],
    ["btc@depth1", {}],
  ]) {
    const existingSymbolTruthy = Boolean(data.s);
    const { o: streamOracle, trace: streamTrace } = fixture(Public);
    for (const [method, key] of [
      ["handleTickerUpdate", "24hrTicker"],
      ["handleTradeUpdate", "aggTrade"],
      ["handleMarkPriceUpdate", "markPriceUpdate"],
      ["handleKlineUpdate", "kline"],
      ["handleDepthSnapshot", "depthUpdate"],
    ]) {
      streamOracle[method] = value => streamTrace.push([key, value.s ?? null]);
    }
    streamOracle.handleStreamData(stream, data);
    streamDispatch.push({ stream, data, existingSymbolTruthy, trace: streamTrace, eventType: streamOracle.lookupCapture.eventType });
  }
  const surrogateStream = JSON.parse('"\\ud800@ticker"');
  const surrogateFixture = fixture(Public);
  surrogateFixture.o.handleTickerUpdate = data =>
    surrogateFixture.trace.push(["24hrTicker", typeof data.s, JSON.stringify(data.s)]);
  surrogateFixture.o.handleStreamData(surrogateStream, {});
  const surrogateDispatch = {
    inputJson: JSON.stringify({ stream: surrogateStream }),
    streamType: typeof surrogateStream,
    trace: surrogateFixture.trace,
  };
  const surrogateObservabilitySupplement = [
    ["lone-surrogate", JSON.parse('"\\ud800@ticker"')],
    ["scalar", "A@ticker"],
    ["paired-surrogate", JSON.parse('"\\ud83d\\ude00@ticker"')],
    ["literal-backslash", String.raw`\ud800@ticker`],
  ].map(([name, stream]) => {
    const { o, trace } = fixture(Public);
    let dispatchedHandler;
    let forwarded;
    o.handleTickerUpdate = data => {
      dispatchedHandler = "handleTickerUpdate";
      const symbol = data.s;
      forwarded = {
        type: typeof symbol,
        length: symbol.length,
        codeUnits: Array.from({ length: symbol.length }, (_, i) => symbol.charCodeAt(i)),
        json: JSON.stringify(symbol),
      };
      trace.push(["handler", "24hrTicker"]);
    };
    o.handleStreamData(stream, {});
    return {
      name,
      streamJson: JSON.stringify(stream),
      dispatchedHandler,
      trace,
      forwarded,
    };
  });
  let { o, trace } = fixture(Public);
  await o.unsubscribe(["btc@depth20@100ms", "eth@aggTrade"]);
  evidence.push({ case: "unsubscribe", frame: trace[0][1], frameText: trace[0][2], nextId: o.subscriptionId });
  ({ o, trace } = fixture(Public));
  o.ws.readyState = 3;
  await o.unsubscribe(["btc@aggTrade"]);
  assert.deepEqual(trace, []);
  evidence.push({ case: "unsubscribe disconnected", trace, nextId: o.subscriptionId });

  ({ o, trace } = fixture(Public));
  await o.subscribe(["btc@unknown"]);
  assert.deepEqual(
    o.topics.unknown,
    ["btc"],
    "ordinary unknown topic is allocated",
  );
  ({ o, trace } = fixture(Public));
  o.onMessage({ data: JSON.stringify([{ e: "unknown" }, { e: "aggTrade" }]) });
  assert.deepEqual(
    trace,
    ["received", ["stats", "aggTrade"], "handleTradeUpdate"],
    "ordinary unknown public event preserves the later batch item",
  );
  evidence.push({ case: "public batch ordinary", trace });
  ({ o, trace } = fixture(Public));
  o.onMessage({ data: JSON.stringify({ e: "aggTrade" }) });
  assert.deepEqual(
    trace,
    ["received", ["stats", "aggTrade"], "handleTradeUpdate"],
    "ordinary public event dispatches its own handler",
  );
  evidence.push({ case: "public event", eventType: "aggTrade", trace });
  process.stdout.write(JSON.stringify({ evidence, streamDispatch, surrogateDispatch, surrogateObservabilitySupplement, listenKeyExpired }));
})().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});
