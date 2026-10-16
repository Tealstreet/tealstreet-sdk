"use strict";
const fs = require("node:fs"),
  path = require("node:path"),
  vm = require("node:vm"),
  assert = require("node:assert/strict");
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, "compiler path required");
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root =
  path.resolve(__dirname, "../../../packages/safe-cex/src/exchanges/cryptocom");
const files = [
  ["exchange", "cryptocom.exchange.ts", "CryptocomExchange"],
  ["public", "cryptocom.ws-public.ts", "CryptocomPublicWebSocket"],
  ["private", "cryptocom.ws-private.ts", "CryptocomPrivateWebSocket"],
];
(async () => {
  const rows = [];
  for (const [side, file, className] of files) {
    const mutation = process.env.SAFE_CEX_CRYPTOCOM_BRIDGE_SOURCE_MUTATION;
    assert(!mutation || mutation === "getAccount", "known source control");
    const source = ts.createSourceFile(
      file,
      fs.readFileSync(path.join(root, file), "utf8"),
      ts.ScriptTarget.Latest,
      true,
    );
    const klass = source.statements.find(
      (n) => ts.isClassDeclaration(n) && n.name.text === className,
    );
    const imports = source.statements
      .filter(ts.isImportDeclaration)
      .flatMap((n) => {
        const clause = n.importClause;
        if (!clause) return [];
        return [
          ...(clause.name ? [clause.name.text] : []),
          ...(clause.namedBindings && ts.isNamedImports(clause.namedBindings)
            ? clause.namedBindings.elements.map((n) => n.name.text)
            : []),
        ];
      });
    const localConstants = source.statements
      .filter(ts.isVariableStatement)
      .flatMap((n) =>
        n.declarationList.declarations.map((n) => n.name.getText(source)),
      );
    const oldNative = new Set(
      side === "exchange"
        ? ["start", "tick"]
        : [
            "onMessage",
            "handleTickerUpdate",
            "handleBookUpdate",
            "handleTradeUpdate",
            "handleOrderUpdate",
            "handlePositionUpdate",
            "handleBalanceUpdate",
            "listenOHLCV",
            "listenOrderBook",
          ],
    );
    for (const member of klass.members) {
      if (!member.initializer || !ts.isArrowFunction(member.initializer))
        continue;
      const name = member.name.getText(source);
      if (oldNative.has(name)) continue;
      const events = [],
        phase = { opaque: true };
      const fail = (event) => {
        events.push(event);
        throw phase;
      };
      const rootValue = new Proxy(
        {},
        {
          get(_, key) {
            return fail("get:" + String(key));
          },
          set() {
            return true;
          },
        },
      );
      const scope = {};
      for (const key of new Set([
        ...imports,
        ...localConstants,
        "parseFloat",
        "Promise",
        "Error",
        "clearTimeout",
        "setTimeout",
      ]))
        Object.defineProperty(scope, key, {
          get() {
            return fail("global:" + key);
          },
        });
      scope.recordFail = fail;
      const context = vm.createContext({ recordFail: fail });
      if (side === "private" && name === "subscribeToOrderUpdates")
        vm.runInContext(
          "Object.defineProperty(Array.prototype,'filter',{get(){return recordFail('get:filter')}})",
          context,
        );
      const code = ts.transpileModule(
        '(function(bindings){with(bindings){return (function(){\"use strict\";return (' +
          (mutation === name && side === "exchange"
            ? "async()=>undefined"
            : member.initializer.getText(source)) +
          ");}).call(this);}})",
        {
          compilerOptions: {
            target: ts.ScriptTarget.ES2022,
            module: ts.ModuleKind.None,
            alwaysStrict: false,
          },
        },
      ).outputText;
      const fn = vm.runInContext(code, context).call(rootValue, scope);
      const object = new Proxy(
        {},
        {
          get(_, key) {
            return fail("get:" + String(key));
          },
        },
      );
      const array = new Proxy([object], {
        get(target, key, receiver) {
          if (key === Symbol.iterator) {
            events.push("iterator");
            return Array.prototype[Symbol.iterator].bind(target);
          }
          return fail("get:" + String(key));
        },
      });
      const arrayMethods = new Set([
        "sendSubscriptionFrames",
        "registerAndSubscribe",
        "subscribeToTickers",
        "subscribeToMarketData",
        "unsubscribe",
      ]);
      const stringMethods = new Set([
        "subscribeToOrderBook",
        "subscribeToTicker",
        "subscribeToTrades",
        "subscribeToCandlestick",
        "getMarketIdsByIndexInstrument",
        "mapOrderType",
        "mapOrderStatus",
        "mapTimeInForce",
        "mapCryptocomOrderType",
        "mapCryptocomTimeInForce",
        "mapStopTriggerType",
      ]);
      const argument = arrayMethods.has(name)
        ? array
        : stringMethods.has(name)
          ? "UNKNOWN"
          : object;
      let returned, thrown;
      try {
        returned = await fn(argument, undefined, undefined);
      } catch (e) {
        thrown = e;
      }
      if (!["ping", "shouldEnforcePongMissReconnect"].includes(name))
        assert.strictEqual(
          thrown,
          phase,
          "source original failure " + side + ":" + name,
        );
      rows.push({
        side,
        name,
        first: events[0] || "",
        noEffects: events.length === 0,
        returned:
          returned === false
            ? "false"
            : returned === undefined
              ? "undefined"
              : "other",
      });
    }
  }
  const orderSource = ts.createSourceFile("order.ts", fs.readFileSync(path.join(root, "cryptocom.exchange.ts"), "utf8"), ts.ScriptTarget.Latest, true);
  const orderClass = orderSource.statements.find(n => ts.isClassDeclaration(n) && n.name.text === "CryptocomExchange");
  const orderMember = orderClass.members.find(n => n.name?.getText(orderSource) === "mapOrder");
  const orderCode = ts.transpileModule("(" + orderMember.initializer.getText(orderSource) + ")", { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText.replace(/^"use strict";\s*/, "");
  const parserOrder = [];
  for (const [iteration, field] of ["quantity", "ref_price"].entries()) {
    const phase = {}, context = vm.createContext({}), counts = { original: 0, replacement: 0 };
    context.parseFloat = value => { counts.original++; return Number(value); };
    const order = new Proxy({ quantity: "7", ref_price: "11" }, { get(target, key) {
      if (key === field) context.parseFloat = () => { counts.replacement++; return 999; };
      if (key === (field === "quantity" ? "cumulative_quantity" : "status")) throw phase;
      return target[key];
    }});
    assert.throws(() => vm.runInContext(orderCode, context)(order), error => error === phase, "SOURCE-PARSE-FLOAT-STOP");
    assert.equal(counts.original, iteration + 1, "SOURCE-PARSE-FLOAT-CAPTURE");
    assert.equal(counts.replacement, 0, "SOURCE-PARSE-FLOAT-CAPTURE");
    parserOrder.push({ field, ...counts });
    counts.original = 0; counts.replacement = 0;
    context.parseFloat = value => { counts.original++; return Number(value); };
    context.OrderSide = { Sell: "sell" };
    const completed = new Proxy({ quantity: "7", ref_price: "11", update_time: 1 }, { get(target,key) {
      if (key === field) context.parseFloat = () => { counts.replacement++; return 999; };
      return target[key];
    }});
    const rootValue = { mapCryptocomOrderType(){}, mapOrderStatus(){}, mapCryptocomTimeInForce(){} };
    const fn = vm.runInContext("(function(){return (" + orderCode.trim().replace(/;$/, "") + ");})", context).call(rootValue);
    const result = fn(completed);
    assert.equal(result.amount, 7, "SOURCE-PARSE-FLOAT-AMOUNT");
    assert.equal(result.price, field === "quantity" ? 999 : 11, "SOURCE-PARSE-FLOAT-PRICE");
    assert.deepEqual([counts.original,counts.replacement], field === "quantity" ? [1,1] : [2,0], "SOURCE-PARSE-FLOAT-FRESH-CALLEE");
  }
  const initializer = name => {
    const member = orderClass.members.find(n => n.name?.getText(orderSource) === name);
    return ts.transpileModule("(function(){return (" + member.initializer.getText(orderSource) + ")})", { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText;
  };
  const orderSides = [];
  for (const [input, key] of [["BUY", "Buy"], ["SELL", "Sell"]]) {
    const expected = {}, OrderSide = { [key]: expected };
    const receiver = { mapCryptocomOrderType(){}, mapOrderStatus(){}, mapCryptocomTimeInForce(){} };
    const fn = vm.runInNewContext(initializer("mapOrder"), { OrderSide }).call(receiver);
    const result = fn({ quantity: "2", ref_price: "3", side: input, update_time: 1 });
    assert.strictEqual(result.side, expected, "SOURCE-ORDER-SIDE-IMPORT");
    orderSides.push(result.side === expected);
  }
  const longs = [{}, {}, {}], shorts = [{}, {}], crosses = [{}, {}], isolated = {};
  const enumCounts = [0, 0, 0], idSides = [];
  const PositionSide = { get Long(){ return longs[enumCounts[0]++]; }, get Short(){ return shorts[enumCounts[1]++]; } };
  const MarginType = { Isolated: isolated, get Cross(){ return crosses[enumCounts[2]++]; } };
  const positionRoot = {
    xhr: { async post(){ return {data: {data: [{ quantity: "2", cost: "6", instrument_name: "ACTUAL", open_position_pnl: "1", isolation_type: "ISOLATED_MARGIN", update_timestamp_ms: 1 }]}}; } },
    async getPositionMode(){ return "hedged"; },
    store: { markets: [{ symbol: "FAKE", active: true, isSpot: false }] },
    log(){ throw Error("SOURCE-POSITION-UNEXPECTED-CATCH"); }
  };
  const positionFn = vm.runInNewContext(initializer("fetchPositions"), {
    ENDPOINTS: { POSITIONS: "positions" }, PositionSide, MarginType,
    virtualClock: { getCurrentTimeMs(){ return 1; } },
    createPositionId(symbol, side){ idSides.push(side); return idSides.length; },
    uniqBy(rows, key){ return rows.filter((row,index) => rows.findIndex(other => other[key] === row[key]) === index); }
  }).call(positionRoot);
  const positions = await positionFn();
  assert.equal(positions.length, 3, "SOURCE-POSITION-INDEPENDENT-FLOOR");
  assert.deepEqual(enumCounts, [3, 2, 2], "SOURCE-POSITION-ENUM-READ-COUNTS");
  const positionSides = [longs[0], longs[2], shorts[1]].map((value,index) => positions[index].side === value);
  const positionMargins = [isolated, crosses[0], crosses[1]].map((value,index) => positions[index].marginType === value);
  const positionIds = [longs[0], longs[1], shorts[0]].map((value,index) => idSides[index] === value);
  for (const checks of [positionSides, positionMargins, positionIds]) assert.deepEqual(checks, [true,true,true], "SOURCE-POSITION-IMPORT-IDENTITY");
  process.stdout.write(JSON.stringify({ rows, parserOrder, enumProof: { orderSides, enumCounts, positionSides, positionMargins, positionIds } }));
})().catch((e) => {
  process.stderr.write(e.stack);
  process.exit(1);
});
