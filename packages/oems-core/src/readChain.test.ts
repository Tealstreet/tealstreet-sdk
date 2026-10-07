import type { CoreAdapter } from '@tealstreet/script-api-core';
import type { OemsContext } from './context.types';
import type { OemsReadResult } from './prompt';

import { CORE_READ_BINDINGS } from '@tealstreet/script-api-core/src/runtime/readBindings';
import { describe, expect, it } from 'vitest';

import { boundReadResults } from './boundContext';
import { parseOemsPlan } from './parsePlan';
import { buildOemsPrompt } from './prompt';
import { parseReadCalls } from './readTools';

/**
 * The whole seam, end to end, with a fake adapter standing in for the venue.
 *
 * Every link has its own tests. This one exists because the links were built in
 * three separate commits and the failure mode of that is a chain where each
 * piece is correct and the joins are not — a flattened argument the binding
 * rejects, a bounded result that no longer parses, a payload that lands outside
 * the fence.
 */

const context = (): OemsContext => ({
  generatedAt: Date.now(),
  activeAccount: 'main',
  activeSymbol: 'BTCUSDT',
  activeAccounts: ['main'],
  accounts: [{ name: 'main', exchange: 'binance', active: true, testnet: false, connected: true }],
  positions: [],
  orders: [],
  balances: [],
  markets: [],
  fatFingerNotional: 0,
});

/** Returns a venue-shaped payload: far too many candles, and an `info` blob. */
const fakeAdapter = (): CoreAdapter =>
  ({
    getCandles: async () =>
      Array.from({ length: 900 }, (_, i) => ({
        timestamp: 1_700_000_000_000 + i * 60_000,
        open: 60_000 + i,
        high: 60_100 + i,
        low: 59_900 + i,
        close: 60_050 + i,
        volume: 12.5,
        info: { raw: 'x'.repeat(400) },
      })),
  }) as unknown as CoreAdapter;

describe('model output → reads → venue → prompt', () => {
  it('carries a candle request all the way to a fenced, bounded prompt', async () => {
    // 1. What the provider returned, in the schema's own flat shape.
    const plan = parseOemsPlan({
      reply: 'Let me pull the last four hours.',
      steps: [],
      reads: [
        {
          tool: 'getCandles',
          symbol: 'BTCUSDT',
          timeframe: '4h',
          account: 'main',
          symbols: null,
          accounts: null,
          from: null,
          to: null,
          limit: null,
          depth: null,
        },
      ],
    });
    expect(plan.steps).toEqual([]);

    // 2. Validated against the snapshot the model was shown — which is the
    //    BOUNDED one. The caller passes the same object it posted; passing the
    //    raw context here is the bug this step exists to catch.
    const { calls, rejections } = parseReadCalls(plan.reads, context());
    expect(rejections).toEqual([]);
    expect(calls).toEqual([{ tool: 'getCandles', args: { symbol: 'BTCUSDT', timeframe: '4h', account: 'main' } }]);

    // 3. Executed against the SDK adapter, exactly as the browser would.
    const binding = CORE_READ_BINDINGS.getCandles;
    const raw = await binding.invoke(fakeAdapter(), calls[0].args);
    expect(raw).toHaveLength(900);

    // 4. Bounded on the way back.
    const results = boundReadResults([{ tool: 'getCandles', args: calls[0].args, ok: true, data: raw }]);
    const bounded = results[0].data as unknown[];
    expect(bounded.length).toBeLessThan(900);
    expect(JSON.stringify(bounded)).not.toContain('xxxx');
    expect(results[0]).toMatchObject({ truncated: true, kept: 200, total: 900 });
    // Truncation is not a failure and must not be reported as one.
    expect(results[0].error).toBeUndefined();

    // 5. Rendered into the next request — inside the fence, never in the
    //    instructions, which are ours and are cached.
    const built = buildOemsPrompt(
      { context: context(), transcript: [], message: 'how much did btc move', readResults: results },
      'nonce1',
    );
    const begin = built.input.indexOf('-----BEGIN TEALSTREET_nonce1-----');
    const end = built.input.indexOf('-----END TEALSTREET_nonce1-----');
    // The LAST candle's close (60_050 + 899). Bounding keeps the most recent
    // window, so asserting on the first candle would pass only by accident.
    const payloadAt = built.input.indexOf('60949');
    expect(payloadAt).toBeGreaterThan(begin);
    expect(payloadAt).toBeLessThan(end);
    expect(built.instructions).not.toContain('60949');
    expect(built.input.length).toBeLessThan(60_000);

    // 6. And the model is TOLD it is holding a subset. This is the assertion
    //    the chain was missing: steps 4 and 5 both passed while the marker was
    //    written onto a field the renderer prints only for failures, so 200 of
    //    900 candles reached the model dressed as the whole range.
    const markerAt = built.input.indexOf('[TRUNCATED: showing 200 of 900');
    expect(markerAt).toBeGreaterThan(begin);
    expect(markerAt).toBeLessThan(end);
  });

  it('refuses the whole chain when the model names an account it was not shown', async () => {
    const plan = parseOemsPlan({
      reply: 'Checking.',
      steps: [],
      reads: [{ tool: 'getCandles', symbol: 'BTCUSDT', timeframe: '4h', account: 'someone-elses' }],
    });
    const { calls, rejections } = parseReadCalls(plan.reads, context());
    expect(calls).toEqual([]);
    expect(rejections[0].reason).toContain('someone-elses');
  });

  it('keeps a failed read in the prompt so the model can explain it', () => {
    const failed: OemsReadResult = { tool: 'getCandles', args: {}, ok: false, error: 'venue timed out' };
    const built = buildOemsPrompt(
      { context: context(), transcript: [], message: 'go', readResults: boundReadResults([failed]) },
      'n',
    );
    expect(built.input).toContain('FAILED: venue timed out');
  });
});
