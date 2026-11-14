import { readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';

import { describe, expect, it } from 'vitest';
import ts from 'typescript';

/**
 * ENFORCEMENT: a CLI trading command does not write to a venue directly.
 *
 * Every placement in this tree used to be a raw call with whatever the command
 * had computed. So `buy ... at` never rounded its price to the tick, `stop`
 * sent an unrounded trigger and an `amount: 0` infinity stop to the twelve
 * venues that do not read one, `close` sent a raw
 * `Math.abs(position.contracts)`, and nothing anywhere applied the reduce-only
 * clamp. Web, mobile and scripts had all of it, through `createCoreAdapter`.
 *
 * The fix is `prepareCliOrder`, which calls the same `prepareOrder` body the
 * adapter calls. This fails if a placement appears that does not.
 *
 * The placement check is per-CALL, not per-file. A per-file version passed
 * while `CloseCommand` had one wrapped write and one raw one — which is
 * exactly the shape this is meant to catch, since a file with two writes is
 * how the first one gets missed.
 */

const COMMANDS_DIR = path.resolve(__dirname, '../commands');

const placementCalls = (source: string): ts.CallExpression[] => {
  const file = ts.createSourceFile('command.ts', source, ts.ScriptTarget.Latest, true);
  const calls: ts.CallExpression[] = [];
  const visit = (node: ts.Node): void => {
    if (ts.isCallExpression(node)) {
      let callee = node.expression;
      if (ts.isNonNullExpression(callee)) callee = callee.expression;
      const name = ts.isPropertyAccessExpression(callee)
        ? callee.name.text
        : ts.isIdentifier(callee)
          ? callee.text
          : '';
      if (name === 'placeOrder' || name === 'placeOrders') calls.push(node);
    }
    ts.forEachChild(node, visit);
  };
  visit(file);
  return calls;
};

// Trace const payloads back to shared preparation, including an order taken
// from a preflighted array. This permits validation before the first write
// without accepting arbitrary raw payloads or widening the sender allowlist.
const isPrepared = (value: ts.Expression, location: ts.Node, batch: boolean): boolean => {
  if (ts.isCallExpression(value) && ts.isIdentifier(value.expression)) {
    return value.expression.text === (batch ? 'prepareCliOrders' : 'prepareCliOrder');
  }
  if (!ts.isIdentifier(value)) return false;
  for (let scope: ts.Node | undefined = location.parent; scope; scope = scope.parent) {
    if (
      !batch &&
      ts.isForOfStatement(scope) &&
      ts.isVariableDeclarationList(scope.initializer) &&
      scope.initializer.flags & ts.NodeFlags.Const
    ) {
      const binding = scope.initializer.declarations[0]?.name;
      if (binding && ts.isIdentifier(binding) && binding.text === value.text) {
        return isPrepared(scope.expression, scope, true);
      }
    }
    if (ts.isBlock(scope)) {
      for (const statement of scope.statements) {
        if (statement.getStart() >= location.getStart()) break;
        if (!ts.isVariableStatement(statement) || !(statement.declarationList.flags & ts.NodeFlags.Const)) continue;
        for (const declaration of statement.declarationList.declarations) {
          if (ts.isIdentifier(declaration.name) && declaration.name.text === value.text) {
            return !!declaration.initializer && isPrepared(declaration.initializer, declaration, batch);
          }
        }
      }
    }
  }
  return false;
};

const hasPreparedPayload = (call: ts.CallExpression): boolean => {
  const payload = call.arguments[0];
  let callee = call.expression;
  if (ts.isNonNullExpression(callee)) callee = callee.expression;
  const name = ts.isPropertyAccessExpression(callee) ? callee.name.text : callee.getText();
  return !!payload && isPrepared(payload, call, name === 'placeOrders');
};

/** Amends carry only a price, and the value is nested inside an object
 *  literal, so this one stays per-file. */
const AMEND_CALL = /\bupdateOrder[!?]?\s*\(/;

/**
 * Commands that hand the whole operation to a lower-level manager rather than
 * placing an order themselves. Their ENTRY sizes still come from
 * `TradingBaseCommand`, so they inherit the sizing; what they do not have is a
 * single order payload to round.
 */
const DELEGATES_TO_A_MANAGER = new Set<string>([
  'trading/NukeCommand.ts',
  'trading/TwapCommand.ts',
  'trading/ChaseCommand.ts',
]);

const walk = (dir: string, out: string[] = []): string[] => {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) walk(full, out);
    else if (entry.name.endsWith('.ts') && !entry.name.endsWith('.test.ts')) out.push(full);
  }
  return out;
};

const sourceFiles = (): readonly { rel: string; source: string }[] =>
  walk(COMMANDS_DIR)
    .map((file) => ({ rel: path.relative(COMMANDS_DIR, file), source: readFileSync(file, 'utf8') }))
    .filter(({ rel }) => !DELEGATES_TO_A_MANAGER.has(rel));

const HOW = (verb: string, call: string): string =>
  `A CLI command ${verb} without the shared order policy sends unrounded ` +
  'prices and sizes to the venue, and skips the minimum-size refusal, the ' +
  'infinity-stop check and the reduce-only clamp that web, mobile and ' +
  `scripts all get. Wrap the payload: ${call}`;

describe('CLI commands write through the shared order policy', () => {
  it('every placement call uses a payload prepared by the shared order policy', () => {
    const offenders: string[] = [];
    for (const { rel, source } of sourceFiles()) {
      for (const call of placementCalls(source)) {
        if (!hasPreparedPayload(call)) {
          const line = source.slice(0, call.getStart()).split('\n').length;
          offenders.push(`${rel}:${line}`);
        }
      }
    }
    expect(offenders.sort(), HOW('placing an order', '`prepareCliOrder(s)(exchange, payload)`')).toEqual([]);
  });

  it('every command that amends an order rounds the new price', () => {
    const offenders = sourceFiles()
      .filter(({ source }) => AMEND_CALL.test(source) && !source.includes('prepareCliAmendPrice'))
      .map(({ rel }) => rel)
      .sort();
    expect(offenders, HOW('amending an order', '`prepareCliAmendPrice(exchange, symbol, price)`')).toEqual([]);
  });

  it('the detectors match real calls and ignore guards, declarations and comments', () => {
    expect(placementCalls('await exchange.placeOrder!(prepareCliOrder(exchange, order))')).toHaveLength(1);
    expect(placementCalls('await placeOrder(order)')).toHaveLength(1);
    expect(placementCalls('if (!exchange.placeOrder) {}')).toHaveLength(0);
    expect(placementCalls('const placeOrder = exchange.placeOrder;')).toHaveLength(0);
    expect(placementCalls('// exchange.placeOrder(raw)')).toHaveLength(0);
    expect(AMEND_CALL.test('await ctx.exchange.updateOrder({ order, update })')).toBe(true);
  });

  it.each([
    'exchange.placeOrder(prepareCliOrder(exchange, raw))',
    'exchange.placeOrders(prepareCliOrders(exchange, raw))',
    'const batch = prepareCliOrders(exchange, raw); exchange.placeOrders(batch)',
    'const batch = prepareCliOrders(exchange, raw); for (const order of batch) exchange.placeOrder(order)',
  ])('accepts payloads traced to shared preparation: %s', (source) => {
    const [call] = placementCalls(`async function submit() { ${source} }`);
    expect(hasPreparedPayload(call)).toBe(true);
  });

  it.each([
    'exchange.placeOrder(raw)',
    'const batch = raw; for (const order of batch) exchange.placeOrder(order)',
    'let batch = prepareCliOrders(exchange, raw); exchange.placeOrders(batch)',
    'exchange.placeOrders(batch); const batch = prepareCliOrders(exchange, raw)',
    'const batch = prepareCliOrders(exchange, raw); exchange.placeOrder(batch)',
  ])('rejects payloads without prior shared preparation: %s', (source) => {
    const [call] = placementCalls(`async function submit() { ${source} }`);
    expect(hasPreparedPayload(call)).toBe(false);
  });
});
