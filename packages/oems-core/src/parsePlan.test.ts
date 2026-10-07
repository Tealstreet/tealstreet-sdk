import { OemsPlanError, parseOemsPlan } from './parsePlan';

/** The shape strict mode actually produces: every field present, absent ones null. */
const step = (over: Record<string, unknown>): Record<string, unknown> => ({
  kind: 'close_positions',
  positionScope: null,
  orderScope: null,
  percent: null,
  account: null,
  symbol: null,
  side: null,
  orderType: null,
  amount: null,
  price: null,
  reduceOnly: null,
  orderId: null,
  leverage: null,
  ...over,
});

describe('parseOemsPlan', () => {
  it('accepts a reply with no steps — a question is not an error', () => {
    expect(parseOemsPlan({ reply: 'Which account?', steps: [] })).toEqual({
      reply: 'Which account?',
      steps: [],
      reads: [],
    });
  });

  it('reads the headline case', () => {
    const plan = parseOemsPlan({
      reply: 'Reducing every open position by 30%.',
      steps: [step({ kind: 'reduce_positions', percent: 30 })],
    });
    expect(plan.steps).toEqual([{ kind: 'reduce_positions', scope: {}, percent: 30 }]);
  });

  it('treats null scope fields as "everything"', () => {
    const plan = parseOemsPlan({
      reply: '',
      steps: [step({ kind: 'close_positions', positionScope: { accounts: null, symbols: ['BTCUSDT'], side: 'long' } })],
    });
    expect(plan.steps[0]).toEqual({ kind: 'close_positions', scope: { symbols: ['BTCUSDT'], side: 'long' } });
  });

  it('OMITS absent optional keys rather than setting them undefined', () => {
    // `updateOrder` on the host decides with `Object.hasOwn`, so a key present
    // and undefined counts as sent and the amend is refused as "both".
    const plan = parseOemsPlan({
      reply: '',
      steps: [step({ kind: 'amend_order', account: 'a', orderId: '1', price: 100 })],
    });
    expect(Object.hasOwn(plan.steps[0], 'amount')).toBe(false);
    expect(Object.hasOwn(plan.steps[0], 'price')).toBe(true);
  });

  it('refuses an amend that changes both or neither', () => {
    const both = step({ kind: 'amend_order', account: 'a', orderId: '1', price: 100, amount: 2 });
    const neither = step({ kind: 'amend_order', account: 'a', orderId: '1' });
    expect(() => parseOemsPlan({ reply: '', steps: [both] })).toThrow(OemsPlanError);
    expect(() => parseOemsPlan({ reply: '', steps: [neither] })).toThrow(OemsPlanError);
  });

  it('refuses a place_order missing its side or type', () => {
    expect(() =>
      parseOemsPlan({
        reply: '',
        steps: [step({ kind: 'place_order', account: 'a', symbol: 'BTCUSDT', amount: 1, orderType: 'market' })],
      }),
    ).toThrow(/side is required/);
  });

  it('names the field it refused', () => {
    expect(() => parseOemsPlan({ reply: '', steps: [step({ kind: 'reduce_positions', percent: 'lots' })] })).toThrow(
      /plan\.steps\[0\]\.percent/,
    );
  });

  it('refuses an unknown kind rather than dropping the step', () => {
    expect(() => parseOemsPlan({ reply: '', steps: [step({ kind: 'nuke_everything' })] })).toThrow(/nuke_everything/);
  });

  it('bounds steps and scope lists', () => {
    const many = Array.from({ length: 21 }, () => step({}));
    expect(() => parseOemsPlan({ reply: '', steps: many })).toThrow(/more than 20 steps/);

    const wide = step({
      kind: 'close_positions',
      positionScope: { accounts: Array.from({ length: 51 }, (_, i) => `a${i}`), symbols: null, side: null },
    });
    expect(() => parseOemsPlan({ reply: '', steps: [wide] })).toThrow(/more than 50 entries/);
  });

  it('refuses a non-object plan', () => {
    expect(() => parseOemsPlan('reduce everything')).toThrow(OemsPlanError);
    expect(() => parseOemsPlan(null)).toThrow(OemsPlanError);
  });
});

describe('parseOemsPlan reads', () => {
  it('carries requested reads through untouched, because validating one needs the snapshot', () => {
    const raw = [{ tool: 'getCandles', symbol: 'BTCUSDT', timeframe: '4h', account: 'main' }];
    expect(parseOemsPlan({ reply: 'Looking.', steps: [], reads: raw }).reads).toEqual(raw);
  });

  it('defaults to an empty list when the field is absent', () => {
    expect(parseOemsPlan({ reply: 'ok', steps: [] }).reads).toEqual([]);
  });

  it('rejects a reads field that is not an array', () => {
    expect(() => parseOemsPlan({ reply: 'ok', steps: [], reads: 'getCandles' })).toThrow(OemsPlanError);
  });

  /**
   * A plan that asks for data AND proposes actions computed those actions
   * without the data. Executing them would be acting on the model's own
   * admission that it was reasoning with something missing.
   */
  it('drops steps when reads are present, rather than acting on stale reasoning', () => {
    const parsed = parseOemsPlan({
      reply: 'Checking first.',
      reads: [{ tool: 'getCandles', symbol: 'BTCUSDT', timeframe: '4h', account: 'main' }],
      steps: [{ kind: 'close_positions', positionScope: null }],
    });
    expect(parsed.steps).toEqual([]);
    expect(parsed.reads).toHaveLength(1);
  });
});
