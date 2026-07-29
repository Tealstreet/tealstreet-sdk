import type { ExchangeOptions } from '../types';

import { describe, expect, it } from 'vitest';

import {
  LIGHTER_INTEGRATOR_MAKER_FEE_VALUE,
  LIGHTER_INTEGRATOR_TAKER_FEE_VALUE,
  resolveLighterBuilderFeeConfig,
} from '../exchanges/lighter/lighter.builder-fee';

const createOptions = (overrides?: Partial<ExchangeOptions>): ExchangeOptions =>
  ({
    key: '',
    secret: '',
    ...overrides,
  }) as ExchangeOptions;

describe('resolveLighterBuilderFeeConfig', () => {
  it('uses dynamic mode by default on mainnet', () => {
    const config = resolveLighterBuilderFeeConfig(createOptions());
    expect(config).toEqual({
      integratorAccountIndex: 158385n,
      integratorMakerFee: LIGHTER_INTEGRATOR_MAKER_FEE_VALUE,
      integratorTakerFee: LIGHTER_INTEGRATOR_TAKER_FEE_VALUE,
      mode: 'dynamic',
    });
  });

  it('returns undefined on testnet until integrator index is configured', () => {
    const config = resolveLighterBuilderFeeConfig(
      createOptions({
        testnet: true,
      }),
    );
    expect(config).toBeUndefined();
  });

  it('uses dynamic mode with maker-free and taker 2.5 bps fee values', () => {
    const config = resolveLighterBuilderFeeConfig(
      createOptions({
        extra: {
          lighterIntegratorAccountIndex: '1234',
          lighterBuilderFeeMode: 'dynamic',
        },
      }),
    );

    expect(config).toEqual({
      integratorAccountIndex: 1234n,
      integratorMakerFee: LIGHTER_INTEGRATOR_MAKER_FEE_VALUE,
      integratorTakerFee: LIGHTER_INTEGRATOR_TAKER_FEE_VALUE,
      mode: 'dynamic',
    });
  });

  it('keeps static mode compatibility while using current maker/taker fee values', () => {
    const config = resolveLighterBuilderFeeConfig(
      createOptions({
        extra: {
          lighterIntegratorAccountIndex: '1234',
          lighterBuilderFeeMode: 'static',
        },
      }),
    );

    expect(config).toEqual({
      integratorAccountIndex: 1234n,
      integratorMakerFee: LIGHTER_INTEGRATOR_MAKER_FEE_VALUE,
      integratorTakerFee: LIGHTER_INTEGRATOR_TAKER_FEE_VALUE,
      mode: 'static',
    });
  });
});
