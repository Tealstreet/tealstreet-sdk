import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';

import ts from 'typescript';
import { describe, expect, it } from 'vitest';

/**
 * ENFORCEMENT: the set of places in the V3 STACK that can send an order is
 * CLOSED.
 *
 * Every one of them decides a price and a size, so every one is somewhere the
 * numbers can be wrong — and over this epic six of them were. A `scale` ladder
 * went out with a NaN price; a chaser started from eight web call sites that
 * each skipped `supportsChaser`, fat-finger and the rounding; every CLI
 * trading command wrote unrounded; `createStopLossOrder` rounded nothing at
 * all; mobile closed positions in the wrong unit. None of that was findable by
 * reading one file, because nothing said what the whole set WAS.
 *
 * This says it. Adding a file that calls `placeOrder` fails the build, and the
 * fix is either to route through an existing entry point or to add it here
 * with the reason it is one.
 *
 * It is a LIST, not a rule about how to write an order. The per-runtime rules
 * are enforced separately and closer to their code:
 *   - `apps/web/src/order/chaseOrder.guard.test.ts` — one chaser entry point
 *   - `packages/cli-core/src/order/cliOrdersGoThroughPolicy.test.ts` — every
 *     CLI placement goes through the shared policy
 *   - `src/mobile/components/orderPathGuards.test.ts` in `tealstreet-mobile` —
 *     the same two rules for its hand-rolled screens, which no guard in THIS
 *     repo can see
 */

const REPO_ROOT = path.resolve(__dirname, '../../../..');
// The public SDK mirror ships no apps/web, so the host files these name are absent there.
// package.json, not the directory: the mirror's CI can leave a generated apps/web/ behind.
const HAS_WEB_APP = existsSync(path.join(REPO_ROOT, 'apps/web/package.json'));

/** Direct calls retain the textual guard. Captured sends require a const binding
 * from an order method, invoked with its original receiver and an order argument.
 * Destructuring, computed properties and indirect alias chains remain out of scope. */
const placesAnOrder = (source: string): boolean => {
  if (/\.placeOrders?[!?]?\s*\(/.test(source)) return true;
  if (!/\.call\s*\(/.test(source)) return false;

  const fileName = 'order-entry.ts';
  const file = ts.createSourceFile(fileName, source, ts.ScriptTarget.Latest, true);
  const host = ts.createCompilerHost({});
  host.getSourceFile = (name) => (name === fileName ? file : undefined);
  const program = ts.createProgram([fileName], { noLib: true, noResolve: true }, host);
  const checker = program.getTypeChecker();
  let found = false;
  const visit = (node: ts.Node): void => {
    if (
      ts.isCallExpression(node) &&
      ts.isPropertyAccessExpression(node.expression) &&
      node.expression.name.text === 'call' &&
      ts.isIdentifier(node.expression.expression) &&
      node.arguments.length >= 2
    ) {
      const binding = checker.getSymbolAtLocation(node.expression.expression)?.valueDeclaration;
      if (
        binding &&
        ts.isVariableDeclaration(binding) &&
        ts.isVariableDeclarationList(binding.parent) &&
        (binding.parent.flags & ts.NodeFlags.Const) !== 0 &&
        binding.initializer &&
        ts.isPropertyAccessExpression(binding.initializer) &&
        /^(placeOrder|placeOrders)$/.test(binding.initializer.name.text)
      ) {
        const receiver = binding.initializer.expression;
        const invokedReceiver = node.arguments[0];
        // Sibling visits must never erase an earlier order send.
        found ||=
          ts.isIdentifier(receiver) &&
          ts.isIdentifier(invokedReceiver) &&
          receiver.text === invokedReceiver.text &&
          checker.getSymbolAtLocation(receiver) === checker.getSymbolAtLocation(invokedReceiver);
      }
    }
    if (!found) ts.forEachChild(node, visit);
  };
  visit(file);
  return found;
};

/**
 * Comments are stripped before matching, because a file that only MENTIONS
 * `exchange.placeOrder()` in its documentation is not an order path.
 * `handleGrvtBuilderUnauthorizedError.ts` matched on its own doc comment and
 * was listed as an entry point on that basis — which also meant the
 * "still places an order" check below was asserting against prose.
 */
const withoutComments = (source: string): string =>
  source.replace(/\/\*[\s\S]*?\*\//g, '').replace(/(^|[^:])\/\/.*$/gm, '$1');

/**
 * The venue implementations. These DEFINE `placeOrder` — they are the floor
 * every entry point below eventually reaches, not callers of an SDK.
 *
 * Only `exchanges/`. `webworker/` is NOT excluded: `TwapInstance` and
 * `ChaserOrderManager` live there and genuinely place orders, repeatedly, so
 * they are named below like every other entry point. Excluding the directory
 * would have hidden them behind a rule about where a file sits rather than
 * what it does.
 */
const isVenueImplementation = (file: string): boolean =>
  file.startsWith('packages/safe-cex/src/exchanges/') ||
  // Under the Rust engine the TS venue adapters move to a test-only oracle here.
  file.startsWith('packages/safe-cex/src/__tests__/reference/');

// Frozen venue adapters under __tests__ are replay fixtures, never runtime entry points.
const isTest = (file: string): boolean => file.includes('/__tests__/') || /\.(test|cy)\.tsx?$/.test(file);

/**
 * Everything that may send an order, and why it is allowed to.
 *
 * The web primitives are the UI path: they add toasts, `playOrderBeep`,
 * optimistic ids and the fat-finger modal on top of the shared rounding. The
 * CLI commands are the headless path and reach the same policy through
 * `prepareCliOrder`. Neither is redundant — a fat-finger modal is right for a
 * human at a button and wrong for a pipeline.
 */
const ENTRY_POINTS: Readonly<Record<string, string>> = Object.freeze({
  // The web UI order path. createMarketOrder.ts and createLimitOrder.ts used
  // to be listed here and each sent to the venue itself; dcc41e7d5f moved
  // that send into sendOrder.ts (client order ids) and both now go through it,
  // so sendOrder.ts is the entry point and they are not. createStopLossOrder.ts
  // followed on 2026-10-11 (91018c546d): a stop now mints its client order id
  // and sends through sendOrder.ts too, so its ghost can be paired by cloid.
  'apps/web/src/order/sendOrder.ts':
    'the web send with a client order id — createMarketOrder, createLimitOrder and createStopLossOrder delegate here',
  'apps/web/src/order/createConditionalOrder.ts': 'web conditional orders',
  'apps/web/src/order/createScaledOrder.ts': 'web scaled ladders',

  // The CLI. Each of these prepares its payload with `prepareCliOrder`,
  // which is NOT listed — it rounds and validates, it does not send.
  // At b861253bb3 (#3449), buy/sell still send in BaseBuySellCommand.submitOrders.
  // d0e3206005e changed that send to captured placeOrder.call(exchange, order);
  // the detector must recognize the call, and this real sender must stay listed.
  'packages/cli-core/src/commands/trading/BaseBuySellCommand.ts': 'buy / sell through submitOrders',
  'packages/cli-core/src/commands/trading/CloseCommand.ts': 'close',
  'packages/cli-core/src/commands/trading/ScaleCommand.ts': 'scale',
  'packages/cli-core/src/commands/trading/StopCommand.ts': 'stop',
  'packages/cli-core/src/commands/trading/BreakevenCommand.ts': 'be — a reduce-only stop at the break-even price',
  'packages/cli-core/src/commands/trading/TrailCommand.ts': 'trail — a trailing stop on the open position',
  'packages/cli-core/src/commands/trading/SwarmCommand.ts': 'swarm',

  // Lower-level managers and first-party callers.
  'packages/script-api-core/src/proxy/InProcessProxy.ts': 'the SDK’s own proxy seam',
  'apps/web/src/components/agent/executeOemsCommands.ts': 'the Agent, through createWebCoreAdapter',
  'packages/safe-cex/src/crossTrade/runtime/crossTradePlaceOrder.ts': 'the cross-trade copier',
  'packages/exchange-mcp/src/tools.ts': 'the MCP server’s place_order, prepared with `prepareCliOrder`',

  /**
   * The long-running managers. A chaser replaces its order up to 1000 times
   * and a TWAP fires a slice per interval, so these place FAR more orders than
   * anything above — but each is one order the caller already authorised,
   * re-priced. Their ENTRY sizes come from the entry points above.
   *
   * Neither rounds its own replacements. That is a known gap, not a hidden
   * one: see the note in `apps/web/src/order/CLAUDE.md`.
   */
  'packages/safe-cex/src/webworker/twap/TwapInstance.ts': 'a TWAP slice',
  'packages/safe-cex/src/webworker/chaserOrder/ChaserOrderManager.ts': 'a chaser replacement',
  'apps/cli/src/cross-trade-daemon.ts': 'the CLI half of the cross-trade copier',
  'scripts/keys/lib/audit-runtime.ts': 'the `yarn keys` live audit',
  'apps/web/src/components/customScripts/sandbox/escapeAttempts.ts':
    'the sandbox escape corpus — attempts, never executed',
});

/**
 * `v2/` is OUT OF SCOPE, explicitly rather than by oversight.
 *
 * It is the vendored legacy frontend — 5,224 tracked files with its own
 * `ChaserInstance` and `TwapInstance` that place their own orders, its own
 * TypeScript version and its own lockfile. None of it consumes this SDK, so
 * "route it through an existing entry point" is not advice that applies there;
 * folding it in would mean porting v2 onto the v3 order path, which is a
 * product decision and not a guard.
 *
 * So the claim this file makes is about the V3 stack, and saying so is the
 * point — an unexplained filter would read as "we checked everything".
 *
 * (It is also absent from some worktrees while present in the main checkout,
 * so the filter is what makes this test behave identically in both.)
 */
const trackedFiles = (): readonly string[] =>
  execFileSync('git', ['ls-files', '*.ts', '*.tsx'], { cwd: REPO_ROOT, encoding: 'utf8' })
    .split('\n')
    .filter(Boolean)
    .filter((file) => !file.startsWith('v2/'));

describe('the set of order entry points is closed', () => {
  it('recognizes the listed CLI buy/sell sender without the web host', () => {
    const file = 'packages/cli-core/src/commands/trading/BaseBuySellCommand.ts';
    expect(file in ENTRY_POINTS).toBe(true);
    expect(
      placesAnOrder(withoutComments(readFileSync(path.join(REPO_ROOT, file), 'utf8'))),
      `${file} is listed but no longer places an order`,
    ).toBe(true);
  });

  // Walks the source tree; under CI load this outruns vitest's 5s default.
  it('nothing outside the known set sends an order', () => {
    const offenders = trackedFiles()
      .filter((file) => !isVenueImplementation(file) && !isTest(file) && !(file in ENTRY_POINTS))
      .filter((file) => placesAnOrder(withoutComments(readFileSync(path.join(REPO_ROOT, file), 'utf8'))))
      .sort();

    expect(
      offenders,
      'A new place that sends an order is a new place a price or a size can be ' +
        'wrong. Route it through an existing entry point — `apps/web/src/order/*` ' +
        'for the UI path, `prepareCliOrder` for the CLI, `createCoreAdapter` for ' +
        'anything headless — or add it here with the reason it is its own.',
    ).toEqual([]);
  }, 30_000);

  // Walks the source tree; under CI load this outruns vitest's 5s default.
  it.skipIf(!HAS_WEB_APP)(
    'every listed entry point still exists and still places an order',
    () => {
      // A stale entry is a hole: it lets a file be deleted and a different one
      // added under the same path without the guard noticing.
      const tracked = new Set(trackedFiles());
      for (const file of Object.keys(ENTRY_POINTS)) {
        expect(tracked, `${file} is listed but no longer tracked`).toContain(file);
        expect(
          placesAnOrder(withoutComments(readFileSync(path.join(REPO_ROOT, file), 'utf8'))),
          `${file} is listed but no longer places an order — remove it`,
        ).toBe(true);
      }
    },
    30_000,
  );

  it('test references are excluded while production sources stay in scope', () => {
    expect(isTest('packages/safe-cex/src/__tests__/reference/coinbase/coinbase.exchange.ts')).toBe(true);
    expect(isTest('packages/safe-cex/src/exchanges/rust/rust.exchange.ts')).toBe(false);
    expect(isTest('packages/script-api-core/src/proxy/InProcessProxy.ts')).toBe(false);
  });

  it('rejects a no-op named placeOrder while recognizing a real captured send', () => {
    expect(placesAnOrder('const placeOrder = async () => [] as string[]; await placeOrder.call(exchange);')).toBe(
      false,
    );
    expect(placesAnOrder('const placeOrder = exchange.placeOrder; await placeOrder.call(exchange, order);')).toBe(true);
  });

  it.each([
    [
      'different identifier receiver',
      'const send = exchange.placeOrder;',
      'send.call(exchange, order)',
      'send.call(other, order)',
    ],
    [
      'property receiver',
      'const send = exchange.placeOrder; const nested = ctx.exchange.placeOrder;',
      'send.call(exchange, order)',
      'nested.call(ctx.exchange, order)',
    ],
  ])('preserves a real send among rejected siblings: %s', (_name, declarations, realSend, rejectedSend) => {
    for (const calls of [
      [realSend, rejectedSend],
      [rejectedSend, realSend],
    ]) {
      const source = `${declarations} await Promise.all([${calls.join(', ')}]);`;
      expect(placesAnOrder(source), source).toBe(true);
    }
  });

  it('requires the captured method, matching receiver, payload and lexical binding', () => {
    for (const source of [
      'const placeOrder = async () => []; await placeOrder.call(exchange, order);',
      'const send = exchange.cancelOrder; await send.call(exchange, order);',
      'const send = exchange.placeOrder; await send.call(otherExchange, order);',
      'const send = exchange.placeOrder; await send.call(exchange);',
      'const send = exchange.placeOrder;',
      'let send = exchange.placeOrder; send = async () => []; await send.call(exchange, order);',
      'const send = exchange.placeOrder; { const send = async () => []; await send.call(exchange, order); }',
      'const exchange = original; const send = exchange.placeOrder; { const exchange = other; await send.call(exchange, order); }',
    ]) {
      expect(placesAnOrder(source), source).toBe(false);
    }
    const file = 'packages/cli-core/src/commands/trading/BaseBuySellCommand.ts';
    const source = withoutComments(readFileSync(path.join(REPO_ROOT, file), 'utf8'));
    expect(placesAnOrder(source.replace('placeOrder.call(exchange, order)', 'Promise.resolve([])'))).toBe(false);
    expect(
      placesAnOrder(
        source
          .replace('const placeOrder = exchange.placeOrder;', 'const placeOrder = async () => [] as string[];')
          .replace('placeOrder.call(exchange, order)', 'placeOrder.call(exchange)'),
      ),
    ).toBe(false);
  });

  it('the detector matches a call and ignores a declaration or a mention', () => {
    expect(placesAnOrder(withoutComments('// calls exchange.placeOrder() on retry'))).toBe(false);
    expect(placesAnOrder(withoutComments('/** wraps `exchange.placeOrders()` */'))).toBe(false);
    expect(placesAnOrder('await exchange.placeOrder!(order)')).toBe(true);
    expect(placesAnOrder('await exchange.placeOrders(orders)')).toBe(true);
    expect(placesAnOrder('await placeOrder.call(exchange, order)')).toBe(false);
    expect(placesAnOrder('const send = exchange.placeOrders; await send.call(exchange, orders);')).toBe(true);
    expect(placesAnOrder('const placeOrder = exchange.placeOrder;')).toBe(false);
    expect(placesAnOrder('readonly placeOrder?: (opts: PlaceOrderOpts) => Promise<string[]>;')).toBe(false);
    expect(placesAnOrder('if (!exchange.placeOrder) return;')).toBe(false);
  });
});
