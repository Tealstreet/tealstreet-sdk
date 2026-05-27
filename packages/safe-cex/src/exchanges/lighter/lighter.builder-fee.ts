import type { ExchangeOptions } from '../../types';

export type LighterBuilderFeeMode = 'dynamic' | 'static';

// Placeholder values: replace with real Tealstreet integrator account indexes later.
export const TEALSTREET_LIGHTER_INTEGRATOR_ACCOUNT_INDEX_MAINNET = '158385';
export const TEALSTREET_LIGHTER_INTEGRATOR_ACCOUNT_INDEX_TESTNET =
  'REPLACE_WITH_LIGHTER_TESTNET_INTEGRATOR_ACCOUNT_INDEX';

// Lighter partner fee value unit: fee_value / 1e6 of trade size.
// 300 = 3 bps, 200 = 2 bps.
export const LIGHTER_DYNAMIC_INTEGRATOR_FEE_VALUE = 300;
export const LIGHTER_STATIC_INTEGRATOR_FEE_VALUE = 200;

export type LighterBuilderFeeConfig = {
  integratorAccountIndex: bigint;
  integratorMakerFee: number;
  integratorTakerFee: number;
  mode: LighterBuilderFeeMode;
};

const resolveMode = (value: unknown): LighterBuilderFeeMode => {
  // Default to dynamic mode unless static is explicitly requested.
  return value === 'static' ? 'static' : 'dynamic';
};

const parsePositiveIntegerBigInt = (value: unknown): bigint | undefined => {
  if (typeof value !== 'string') {
    return undefined;
  }
  const trimmed = value.trim();
  if (!/^\d+$/.test(trimmed)) {
    return undefined;
  }

  try {
    const parsed = BigInt(trimmed);
    if (parsed < 0n) {
      return undefined;
    }
    return parsed;
  } catch {
    return undefined;
  }
};

export const resolveLighterBuilderFeeConfig = (options: ExchangeOptions): LighterBuilderFeeConfig | undefined => {
  const mode = resolveMode(options.extra?.lighterBuilderFeeMode);
  const configuredIntegratorIndex = options.extra?.lighterIntegratorAccountIndex;
  const placeholderIntegratorIndex = options.testnet
    ? TEALSTREET_LIGHTER_INTEGRATOR_ACCOUNT_INDEX_TESTNET
    : TEALSTREET_LIGHTER_INTEGRATOR_ACCOUNT_INDEX_MAINNET;

  const integratorAccountIndex = parsePositiveIntegerBigInt(configuredIntegratorIndex ?? placeholderIntegratorIndex);
  if (integratorAccountIndex === undefined) {
    return undefined;
  }

  const feeValue = mode === 'static' ? LIGHTER_STATIC_INTEGRATOR_FEE_VALUE : LIGHTER_DYNAMIC_INTEGRATOR_FEE_VALUE;
  return {
    integratorAccountIndex,
    integratorMakerFee: feeValue,
    integratorTakerFee: feeValue,
    mode,
  };
};
