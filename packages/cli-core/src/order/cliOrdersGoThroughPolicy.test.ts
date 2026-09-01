import { readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';

import { describe, expect, it } from 'vitest';

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

/** A placement call, capturing what it opens with. `if (!exchange.placeOrder)`
 *  and `const placeOrder = exchange.placeOrder;` have no `(` after the name
 *  and do not match. */
const PLACEMENT_CALL = /\bplaceOrders?[!?]?\s*\(\s*([A-Za-z_$]*)/g;

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
  it('every placement call opens with prepareCliOrder(s)', () => {
    const offenders: string[] = [];
    for (const { rel, source } of sourceFiles()) {
      for (const match of source.matchAll(PLACEMENT_CALL)) {
        if (!match[1].startsWith('prepareCli')) {
          const line = source.slice(0, match.index).split('\n').length;
          offenders.push(`${rel}:${line}`);
        }
      }
    }
    expect(offenders.sort(), HOW('placing an order', '`exchange.placeOrder(prepareCliOrder(exchange, order))`')).toEqual(
      [],
    );
  });

  it('every command that amends an order rounds the new price', () => {
    const offenders = sourceFiles()
      .filter(({ source }) => AMEND_CALL.test(source) && !source.includes('prepareCliAmendPrice'))
      .map(({ rel }) => rel)
      .sort();
    expect(offenders, HOW('amending an order', '`prepareCliAmendPrice(exchange, symbol, price)`')).toEqual([]);
  });

  it('the detectors match real calls and ignore the guards beside them', () => {
    const hits = (src: string) => [...src.matchAll(PLACEMENT_CALL)].map((m) => m[1]);
    expect(hits('await exchange.placeOrder!(prepareCliOrder(exchange, order))')).toEqual(['prepareCliOrder']);
    expect(hits('await placeOrder(order)')).toEqual(['order']);
    expect(hits('if (!exchange.placeOrder) {')).toEqual([]);
    expect(hits('const placeOrder = exchange.placeOrder;')).toEqual([]);
    expect(AMEND_CALL.test('await ctx.exchange.updateOrder({ order, update })')).toBe(true);
  });
});
