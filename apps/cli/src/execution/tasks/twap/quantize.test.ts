import { describe, expect, it } from 'vitest';
import {
  deriveTwapQuantityConstraints,
  exactQuantityFromAtoms,
  parseUnsignedDecimal,
  quantizeTwapTarget,
  twapMetadataFromMarket,
  type StoreMarketLike,
  type TwapMarketMetadata,
} from './quantize';

/** Bybit ETHUSDT linear perp in the shape produced by `fetchMarketsHelper`. */
const bybitEthUsdt: StoreMarketLike = {
  id: 'ETHUSDT',
  symbol: 'ETHUSDT',
  quote: 'USDT',
  precision: { amount: 0.01 },
  limits: { amount: { min: 0.01 } },
  info: {
    symbol: 'ETHUSDT',
    baseCoin: 'ETH',
    quoteCoin: 'USDT',
    status: 'Trading',
    lotSizeFilter: {
      qtyStep: '0.01',
      minOrderQty: '0.01',
      minNotionalValue: '5',
    },
  },
};

/** Binance USD-M after safe-cex stores a normalized ID beside the exact venue symbol. */
const binanceEthUsdt: StoreMarketLike = {
  id: 'ETH/USDT:USDT',
  symbol: 'ETHUSDT',
  quote: 'USDT',
  precision: { amount: 0.001 },
  limits: { amount: { min: 0.001 } },
};

/** OKX ETH-USDT-SWAP after the safe-cex market mapper has normalized its store symbol and amount minimum. */
const okxEthUsdt: StoreMarketLike = {
  id: 'ETH-USDT-SWAP',
  symbol: 'ETHUSDT',
  quote: 'USDT',
  contractSize: 0.1,
  precision: { amount: 1 },
  limits: { amount: { min: 0.1 } },
  info: {
    instId: 'ETH-USDT-SWAP',
    ctType: 'linear',
    ctVal: '0.1',
    lotSz: '1',
    minSz: '1',
  },
};

/** Bitget Classic ETHUSDT after the safe-cex market mapper, retaining the raw quote floor. */
const bitgetEthUsdt: StoreMarketLike = {
  id: 'ETHUSDT',
  symbol: 'ETHUSDT',
  quote: 'USDT',
  precision: { amount: 0.01 },
  limits: { amount: { min: 0.01 } },
  info: {
    symbol: 'ETHUSDT',
    baseCoin: 'ETH',
    quoteCoin: 'USDT',
    sizeMultiplier: '0.01',
    minTradeNum: '0.01',
    minTradeUSDT: '5',
  },
};

/** BloFin ETH-USDT after the safe-cex market mapper has converted the minimum to base. */
const blofinEthUsdt: StoreMarketLike = {
  id: 'ETH-USDT',
  symbol: 'ETHUSDT',
  quote: 'USDT',
  contractSize: 0.1,
  precision: { amount: 1 },
  limits: { amount: { min: 0.1 } },
  info: {
    instId: 'ETH-USDT',
    contractValue: 0.1,
    lotSize: 1,
    minSize: '1',
  },
};

const metadata = (overrides: Partial<TwapMarketMetadata> = {}): TwapMarketMetadata => ({
  symbol: 'ETHUSDT',
  quote: 'USDT',
  quantityStep: '0.01',
  minQuantity: '0.01',
  contractSize: '1',
  ...overrides,
});

describe('parseUnsignedDecimal', () => {
  it('splits a decimal into atoms and scale, trailing zeros included', () => {
    expect(parseUnsignedDecimal('0.010')).toEqual({ atoms: 10n, atomScale: 3 });
    expect(parseUnsignedDecimal('4000')).toEqual({ atoms: 4000n, atomScale: 0 });
  });

  it('refuses anything that is not an unsigned decimal', () => {
    for (const value of ['-1', '1.', '.5', '01', '1e3', '', ' 1']) {
      expect(parseUnsignedDecimal(value)).toBeUndefined();
    }
  });
});

describe('exactQuantityFromAtoms', () => {
  it('renders atoms at scale without losing the leading zero', () => {
    expect(exactQuantityFromAtoms(125n, 2)).toEqual({ decimal: '1.25', atoms: '125', atomScale: 2 });
    expect(exactQuantityFromAtoms(5n, 3)).toEqual({ decimal: '0.005', atoms: '5', atomScale: 3 });
    expect(exactQuantityFromAtoms(0n, 0)).toEqual({ decimal: '0', atoms: '0', atomScale: 0 });
  });
});


describe('twapMetadataFromMarket', () => {
  it.each([
    ['OKX', okxEthUsdt],
    ['BloFin', blofinEthUsdt],
  ] as const)('keeps %s scheduling in base while normalizing its contract step', (_venue, market) => {
    const projected = twapMetadataFromMarket(market, '2000');
    const result = quantizeTwapTarget({
      metadata: projected,
      target: { kind: 'base', quantity: '0.5' },
      sliceCount: 5,
    });

    expect(result).toEqual({
      kind: 'quantized',
      atomScale: 1,
      sliceCount: 5,
      target: { decimal: '0.5', atoms: '5', atomScale: 1 },
      quantityStep: { decimal: '0.1', atoms: '1', atomScale: 1 },
      minQuantity: { decimal: '0.1', atoms: '1', atomScale: 1 },
      contractSize: { decimal: '0.1', atoms: '1', atomScale: 1 },
      minSliceQuantity: { decimal: '0.1', atoms: '1', atomScale: 1 },
      smallestSliceQuantity: { decimal: '0.1', atoms: '1', atomScale: 1 },
    });
    expect(projected.symbol).toBe(market.symbol);
  });

  it('ignores Binance normalized store IDs when projecting the venue symbol and quote', () => {
    const projected = twapMetadataFromMarket(binanceEthUsdt, '2000');

    expect(projected).toMatchObject({ symbol: 'ETHUSDT', quote: 'USDT' });
    expect(
      quantizeTwapTarget({
        metadata: projected,
        target: { kind: 'base', quantity: '0.005' },
        sliceCount: 5,
      }),
    ).toMatchObject({ kind: 'quantized', sliceCount: 5 });
  });

  it.each([
    ['Bybit', bybitEthUsdt],
    ['Bitget', bitgetEthUsdt],
  ] as const)('enforces the %s raw instrument minimum-notional floor', (_venue, market) => {
    expect(deriveTwapQuantityConstraints(twapMetadataFromMarket(market, '100'))).toEqual({
      atomScale: 2,
      quantityStep: { decimal: '0.01', atoms: '1', atomScale: 2 },
      minQuantity: { decimal: '0.01', atoms: '1', atomScale: 2 },
      contractSize: { decimal: '1', atoms: '1', atomScale: 0 },
      minSliceQuantity: { decimal: '0.05', atoms: '5', atomScale: 2 },
    });
  });
});


describe('deriveTwapQuantityConstraints', () => {
  it('rejects a venue minimum with finer precision than the quantity step', () => {
    expect(deriveTwapQuantityConstraints(metadata({ quantityStep: '0.1', minQuantity: '0.01' }))).toEqual({
      kind: 'rejected',
      code: 'market-metadata-missing',
      reason: 'ETHUSDT minimum quantity has finer precision than the quantity step',
    });
  });

  it('expresses a coarser venue minimum at the quantity step scale', () => {
    expect(deriveTwapQuantityConstraints(metadata({ quantityStep: '0.001', minQuantity: '0.01' }))).toEqual({
      atomScale: 3,
      quantityStep: { decimal: '0.001', atoms: '1', atomScale: 3 },
      minQuantity: { decimal: '0.010', atoms: '10', atomScale: 3 },
      contractSize: { decimal: '1', atoms: '1', atomScale: 0 },
      minSliceQuantity: { decimal: '0.010', atoms: '10', atomScale: 3 },
    });
  });


  it('expands exponential metadata numbers instead of failing to parse them', () => {
    expect(deriveTwapQuantityConstraints(metadata({ quantityStep: 1e-7, minQuantity: 1e-7 }))).toMatchObject({
      atomScale: 7,
      quantityStep: { decimal: '0.0000001', atoms: '1' },
    });
  });
});

describe('quantizeTwapTarget', () => {
  it('matches the live Bybit ETHUSDT preflight sizing for 5000 USDT across 10 slices', () => {
    const result = quantizeTwapTarget({
      metadata: twapMetadataFromMarket(bybitEthUsdt, '4000'),
      target: { kind: 'notional', quoteNotional: '5000' },
      sliceCount: 10,
    });

    expect(result).toEqual({
      kind: 'quantized',
      atomScale: 2,
      sliceCount: 10,
      target: { decimal: '1.25', atoms: '125', atomScale: 2 },
      quantityStep: { decimal: '0.01', atoms: '1', atomScale: 2 },
      minQuantity: { decimal: '0.01', atoms: '1', atomScale: 2 },
      contractSize: { decimal: '1', atoms: '1', atomScale: 0 },
      minSliceQuantity: { decimal: '0.01', atoms: '1', atomScale: 2 },
      smallestSliceQuantity: { decimal: '0.12', atoms: '12', atomScale: 2 },
    });
  });

  it('floors a notional target down to a whole quantity step', () => {
    expect(
      quantizeTwapTarget({
        metadata: metadata({ markPrice: '4000' }),
        target: { kind: 'notional', quoteNotional: '5049' },
        sliceCount: 10,
      }),
    ).toMatchObject({ kind: 'quantized', target: { decimal: '1.26' } });
  });

  it('floors a base target finer than the quantity step', () => {
    expect(
      quantizeTwapTarget({
        metadata: metadata(),
        target: { kind: 'base', quantity: '1.2345' },
        sliceCount: 3,
      }),
    ).toMatchObject({
      kind: 'quantized',
      target: { decimal: '1.23', atoms: '123', atomScale: 2 },
      smallestSliceQuantity: { decimal: '0.41', atoms: '41', atomScale: 2 },
    });
  });

  it('allocates the smallest slice in whole steps when the target does not divide evenly', () => {
    expect(
      quantizeTwapTarget({
        metadata: metadata({ quantityStep: '0.1', minQuantity: '0.1' }),
        target: { kind: 'base', quantity: '1' },
        sliceCount: 3,
      }),
    ).toMatchObject({
      kind: 'quantized',
      target: { decimal: '1.0', atoms: '10', atomScale: 1 },
      smallestSliceQuantity: { decimal: '0.3', atoms: '3', atomScale: 1 },
    });
  });

  it('accepts a target whose smallest slice exactly meets the minimum notional floor', () => {
    expect(
      quantizeTwapTarget({
        metadata: twapMetadataFromMarket(bybitEthUsdt, '100'),
        target: { kind: 'base', quantity: '0.5' },
        sliceCount: 10,
      }),
    ).toMatchObject({
      kind: 'quantized',
      minSliceQuantity: { decimal: '0.05' },
      smallestSliceQuantity: { decimal: '0.05' },
    });
  });

  it('rejects a target whose smallest slice is below the minimum notional floor', () => {
    expect(
      quantizeTwapTarget({
        metadata: twapMetadataFromMarket(bybitEthUsdt, '100'),
        target: { kind: 'base', quantity: '0.3' },
        sliceCount: 10,
      }),
    ).toEqual({
      kind: 'rejected',
      code: 'target-below-min-slice',
      reason: 'target resolves to 0.03 per slice, below the 0.05 ETHUSDT floor',
    });
  });

  it('rejects a target below one venue minimum slice', () => {
    expect(
      quantizeTwapTarget({
        metadata: metadata({ markPrice: '4000' }),
        target: { kind: 'notional', quoteNotional: '10' },
        sliceCount: 10,
      }),
    ).toMatchObject({ kind: 'rejected', code: 'target-below-min-slice' });
  });

  it('rejects missing market metadata', () => {
    const noStep = quantizeTwapTarget({
      metadata: twapMetadataFromMarket({ symbol: 'ETHUSDT', quote: 'USDT' }, '4000'),
      target: { kind: 'notional', quoteNotional: '5000' },
      sliceCount: 10,
    });
    const noMin = quantizeTwapTarget({
      metadata: metadata({ minQuantity: undefined, markPrice: '4000' }),
      target: { kind: 'notional', quoteNotional: '5000' },
      sliceCount: 10,
    });
    const zeroStep = quantizeTwapTarget({
      metadata: metadata({ quantityStep: 0, markPrice: '4000' }),
      target: { kind: 'notional', quoteNotional: '5000' },
      sliceCount: 10,
    });

    for (const result of [noStep, noMin, zeroStep]) {
      expect(result).toMatchObject({ kind: 'rejected', code: 'market-metadata-missing' });
    }
  });

  it('rejects a notional target with no mark price', () => {
    expect(
      quantizeTwapTarget({
        metadata: metadata(),
        target: { kind: 'notional', quoteNotional: '5000' },
        sliceCount: 10,
      }),
    ).toMatchObject({ kind: 'rejected', code: 'market-metadata-missing' });
  });

  it('rejects a minimum notional it cannot convert without a mark price', () => {
    expect(
      quantizeTwapTarget({
        metadata: twapMetadataFromMarket(bybitEthUsdt),
        target: { kind: 'base', quantity: '1' },
        sliceCount: 10,
      }),
    ).toMatchObject({ kind: 'rejected', code: 'market-metadata-missing' });
  });

  it('rejects a non-USDT quote regardless of the label spelling', () => {
    expect(
      quantizeTwapTarget({
        metadata: metadata({ symbol: 'ETHUSDC', quote: 'USDC' }),
        target: { kind: 'base', quantity: '1' },
        sliceCount: 5,
      }),
    ).toEqual({
      kind: 'rejected',
      code: 'symbol-not-usdt-linear',
      reason: 'ETHUSDC is not a linear USDT perpetual market',
    });
  });

  it('accepts USDT market metadata without inferring from the label spelling', () => {
    expect(deriveTwapQuantityConstraints(metadata({ symbol: 'venue-market-42' }))).toMatchObject({
      atomScale: 2,
    });
  });

  it('rejects a USDT-shaped symbol on an inverse or spot market', () => {
    const spot = quantizeTwapTarget({
      metadata: twapMetadataFromMarket({ ...bybitEthUsdt, isSpot: true }, '4000'),
      target: { kind: 'base', quantity: '1' },
      sliceCount: 5,
    });
    const inverse = quantizeTwapTarget({
      metadata: twapMetadataFromMarket({ ...bybitEthUsdt, isInverse: true }, '4000'),
      target: { kind: 'base', quantity: '1' },
      sliceCount: 5,
    });

    expect(spot).toMatchObject({ kind: 'rejected', code: 'symbol-not-usdt-linear' });
    expect(inverse).toMatchObject({ kind: 'rejected', code: 'symbol-not-usdt-linear' });
  });

  it('throws on a slice count that is not a positive integer', () => {
    expect(() =>
      quantizeTwapTarget({ metadata: metadata(), target: { kind: 'base', quantity: '1' }, sliceCount: 0 }),
    ).toThrow(/slice count must be a positive integer/);
  });
  it('allows a slice equal to the finite market maximum and rejects one step above', () => {
    const boundedMarket: StoreMarketLike = {
      ...bybitEthUsdt,
      limits: { amount: { min: 0.01, max: 0.05 } },
    };
    const metadata = twapMetadataFromMarket(boundedMarket, '100');

    expect(metadata.maxQuantity).toBe('0.05');
    expect(
      quantizeTwapTarget({
        metadata,
        target: { kind: 'base', quantity: '0.10' },
        sliceCount: 2,
      }),
    ).toMatchObject({ kind: 'quantized', maxQuantity: { decimal: '0.05' } });
    expect(
      quantizeTwapTarget({
        metadata,
        target: { kind: 'base', quantity: '0.11' },
        sliceCount: 2,
      }),
    ).toMatchObject({ kind: 'rejected', code: 'target-above-max-slice' });
  });

  it('converts a contract-denominated maximum to the base slice ceiling', () => {
    const boundedMarket: StoreMarketLike = {
      ...okxEthUsdt,
      limits: { amount: { min: 0.1, max: 2 } },
    };
    const metadata = twapMetadataFromMarket(boundedMarket, '100');

    expect(metadata.maxQuantity).toBe('0.2');
    expect(
      quantizeTwapTarget({
        metadata,
        target: { kind: 'base', quantity: '0.4' },
        sliceCount: 2,
      }),
    ).toMatchObject({ kind: 'quantized', maxQuantity: { decimal: '0.2' } });
    expect(
      quantizeTwapTarget({
        metadata,
        target: { kind: 'base', quantity: '0.5' },
        sliceCount: 2,
      }),
    ).toMatchObject({ kind: 'rejected', code: 'target-above-max-slice' });
  });

  it('treats a nonfinite published maximum as locally unbounded', () => {
    const metadata = twapMetadataFromMarket(
      {
        ...bybitEthUsdt,
        limits: { amount: { min: 0.01, max: Number.POSITIVE_INFINITY } },
      },
      '100',
    );

    expect(metadata.maxQuantity).toBeUndefined();
    expect(
      quantizeTwapTarget({
        metadata,
        target: { kind: 'base', quantity: '100' },
        sliceCount: 1,
      }),
    ).toMatchObject({ kind: 'quantized' });
  });
});
