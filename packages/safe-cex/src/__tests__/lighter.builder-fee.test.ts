import type { ExchangeOptions } from '../types';

import { describe, expect, it } from 'vitest';

import {
  LIGHTER_DYNAMIC_INTEGRATOR_FEE_VALUE,
  LIGHTER_STATIC_INTEGRATOR_FEE_VALUE,
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
      integratorMakerFee: LIGHTER_DYNAMIC_INTEGRATOR_FEE_VALUE,
      integratorTakerFee: LIGHTER_DYNAMIC_INTEGRATOR_FEE_VALUE,
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

  it('uses dynamic mode with 3 bps fee value', () => {
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
      integratorMakerFee: LIGHTER_DYNAMIC_INTEGRATOR_FEE_VALUE,
      integratorTakerFee: LIGHTER_DYNAMIC_INTEGRATOR_FEE_VALUE,
      mode: 'dynamic',
    });
  });

  it('uses static mode with 2 bps fee value', () => {
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
      integratorMakerFee: LIGHTER_STATIC_INTEGRATOR_FEE_VALUE,
      integratorTakerFee: LIGHTER_STATIC_INTEGRATOR_FEE_VALUE,
      mode: 'static',
    });
  });
});
