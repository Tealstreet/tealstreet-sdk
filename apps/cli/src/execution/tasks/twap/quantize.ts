/**
 * Pure base-quantity derivation for the confirmed market TWAP: venue step,
 * minimum, contract size, and the exact per-slice atoms a target resolves to.
 * No I/O, clock, or venue calls: market metadata and a mark price in, exact
 * quantities or a typed rejection out.
 */

import type { ExactQuantity } from '../../confirmed/types';

/** Venue quantity facts required to size a TWAP. Decimal strings or finite numbers. */
export interface TwapMarketMetadata {
  /** Display and diagnostic label (`market.symbol`); never used to infer market kind. */
  readonly symbol: string;
  /** Normalized settlement quote from venue metadata. */
  readonly quote?: string;
  /** Quantity step in base units. */
  readonly quantityStep?: number | string;
  /** Minimum order quantity in base units. */
  readonly minQuantity?: number | string;
  /** Maximum order quantity in base units. Missing or nonfinite means unbounded locally. */
  readonly maxQuantity?: number | string;
  /** Base units represented by one venue-native contract. Defaults to one. */
  readonly contractSize?: number | string;
  /** Venue minimum order notional in quote units, when the venue publishes one. */
  readonly minNotional?: number | string;
  /** Mark price in quote units. Required for a notional target or a min-notional floor. */
  readonly markPrice?: number | string;
  /** Venue market kind. */
  readonly isInverse?: boolean;
  readonly isSpot?: boolean;
}

/** Structural subset of a safe-cex store market. */
export interface StoreMarketLike {
  /** Store identity; intentionally ignored for eligibility and diagnostics. */
  readonly id?: string;
  /** Exact venue symbol used as the human-readable market label. */
  readonly symbol: string;
  readonly quote?: string;
  readonly contractSize?: number | string;
  readonly precision?: { readonly amount?: number | string };
  readonly limits?: { readonly amount?: { readonly min?: number | string; readonly max?: number | string } };
  readonly info?: Readonly<Record<string, unknown>> & {
    readonly lotSize?: number | string;
    readonly lotSz?: number | string;
    readonly sizeMultiplier?: number | string;
    readonly minSize?: number | string;
    readonly minSz?: number | string;
    readonly minTradeNum?: number | string;
    readonly minTradeUSDT?: number | string;
    readonly lotSizeFilter?: Readonly<Record<string, unknown>> & {
      readonly qtyStep?: number | string;
      readonly minOrderQty?: number | string;
      readonly minNotionalValue?: number | string;
    };
  };
  readonly isInverse?: boolean;
  readonly isSpot?: boolean;
}

/**
 * Project a safe-cex store market onto the canonical base-quantity model.
 * Contract venues publish `precision.amount` and `limits.amount.max` in
 * contracts, while safe-cex exposes `limits.amount.min` in base. Convert the
 * native step, raw minimum, and finite maximum exactly once.
 */
export function twapMetadataFromMarket(market: StoreMarketLike, markPrice?: number | string): TwapMarketMetadata {
  const contractSize = market.contractSize ?? 1;
  const nativeStep =
    market.precision?.amount ??
    market.info?.lotSize ??
    market.info?.lotSz ??
    market.info?.sizeMultiplier ??
    market.info?.lotSizeFilter?.qtyStep;
  const nativeMinimum =
    market.info?.minSize ??
    market.info?.minSz ??
    market.info?.minTradeNum ??
    market.info?.lotSizeFilter?.minOrderQty;
  return {
    symbol: market.symbol,
    quote: market.quote,
    quantityStep: multiplyDecimalValues(nativeStep, contractSize),
    minQuantity: market.limits?.amount?.min ?? multiplyDecimalValues(nativeMinimum, contractSize),
    maxQuantity: multiplyDecimalValues(finitePositiveDecimalValue(market.limits?.amount?.max), contractSize),
    contractSize,
    minNotional: market.info?.lotSizeFilter?.minNotionalValue ?? market.info?.minTradeUSDT,
    markPrice,
    isInverse: market.isInverse,
    isSpot: market.isSpot,
  };
}

/** Requested TWAP size: base quantity, or quote notional converted at the mark. */
export type TwapTarget =
  | { readonly kind: 'base'; readonly quantity: number | string }
  | { readonly kind: 'notional'; readonly quoteNotional: number | string };

export type TwapQuantizeRejectionCode =
  | 'market-metadata-missing'
  | 'target-below-min-slice'
  | 'target-above-max-slice'
  | 'symbol-not-usdt-linear';

export interface TwapQuantizeRejection {
  readonly kind: 'rejected';
  readonly code: TwapQuantizeRejectionCode;
  readonly reason: string;
}

/** Venue quantity constraints expressed in canonical base units. */
export interface TwapQuantityConstraints {
  readonly atomScale: number;
  readonly quantityStep: ExactQuantity;
  readonly minQuantity: ExactQuantity;
  readonly maxQuantity?: ExactQuantity;
  /** Base units represented by one venue-native contract. */
  readonly contractSize: ExactQuantity;
  /** Per-slice floor: the venue minimum raised to satisfy min notional when one is published. */
  readonly minSliceQuantity: ExactQuantity;
}

export interface TwapQuantization {
  readonly kind: 'quantized';
  readonly atomScale: number;
  readonly sliceCount: number;
  /** Base target floored to a whole number of base-denominated quantity steps. */
  readonly target: ExactQuantity;
  readonly quantityStep: ExactQuantity;
  readonly minQuantity: ExactQuantity;
  readonly maxQuantity?: ExactQuantity;
  /** Base units represented by one venue-native contract; renderers convert only at the wire boundary. */
  readonly contractSize: ExactQuantity;
  readonly minSliceQuantity: ExactQuantity;
  /** Smallest base slice the even step allocation produces; never below `minSliceQuantity`. */
  readonly smallestSliceQuantity: ExactQuantity;
}

export type TwapQuantizeResult = TwapQuantization | TwapQuantizeRejection;
export type TwapConstraintsResult = TwapQuantityConstraints | TwapQuantizeRejection;
const UNSIGNED_DECIMAL = /^(?:0|[1-9]\d*)(?:\.\d+)?$/;


/** Split an unsigned decimal string into integer atoms and its atom scale. */
export function parseUnsignedDecimal(value: string): { atoms: bigint; atomScale: number } | undefined {
  if (!UNSIGNED_DECIMAL.test(value)) return undefined;
  const [whole, fraction = ''] = value.split('.');
  return { atoms: BigInt(`${whole}${fraction}`), atomScale: fraction.length };
}

/** Render integer atoms at `atomScale` as an ExactQuantity. */
export function exactQuantityFromAtoms(atoms: bigint, atomScale: number): ExactQuantity {
  const digits = atoms.toString();
  if (atomScale === 0) return { decimal: digits, atoms: digits, atomScale };
  const padded = digits.padStart(atomScale + 1, '0');
  const split = padded.length - atomScale;
  return { decimal: `${padded.slice(0, split)}.${padded.slice(split)}`, atoms: digits, atomScale };
}

/**
 * Base-denominated venue step and minimum expressed at the quantity step's
 * atom scale, with the per-slice floor raised to the venue min notional when
 * one is published. A minimum finer than the step is a metadata contradiction,
 * not a finer scale.
 */
export function deriveTwapQuantityConstraints(metadata: TwapMarketMetadata): TwapConstraintsResult {
  if (metadata.quote !== 'USDT' || metadata.isInverse === true || metadata.isSpot === true) {
    return {
      kind: 'rejected',
      code: 'symbol-not-usdt-linear',
      reason: `${metadata.symbol} is not a linear USDT perpetual market`,
    };
  }
  const contractSize = readDecimal(metadata.contractSize ?? 1);
  if (contractSize === undefined || contractSize.atoms <= 0n) {
    return missing(`${metadata.symbol} contract size is missing or not a positive decimal`);
  }
  const step = readDecimal(metadata.quantityStep);
  if (step === undefined || step.atoms <= 0n) {
    return missing(`${metadata.symbol} quantity step is missing or not a positive decimal`);
  }
  const min = readDecimal(metadata.minQuantity);
  if (min === undefined || min.atoms <= 0n) {
    return missing(`${metadata.symbol} minimum quantity is missing or not a positive decimal`);
  }

  if (min.atomScale > step.atomScale) {
    return missing(`${metadata.symbol} minimum quantity has finer precision than the quantity step`);
  }
  const atomScale = step.atomScale;
  const stepAtoms = step.atoms;
  const minAtoms = rescale(min.atoms, min.atomScale, atomScale);
  const maximum = readDecimal(metadata.maxQuantity);
  const maxQuantity =
    maximum !== undefined && maximum.atoms > 0n
      ? exactQuantityFromAtoms(maximum.atoms, maximum.atomScale)
      : undefined;

  const notionalFloor = notionalFloorAtoms(metadata, stepAtoms, atomScale);
  if (isTwapQuantizeRejection(notionalFloor)) return notionalFloor;

  return {
    atomScale,
    quantityStep: exactQuantityFromAtoms(stepAtoms, atomScale),
    minQuantity: exactQuantityFromAtoms(minAtoms, atomScale),
    ...(maxQuantity === undefined ? {} : { maxQuantity }),
    contractSize: exactQuantityFromAtoms(contractSize.atoms, contractSize.atomScale),
    minSliceQuantity: exactQuantityFromAtoms(minAtoms > notionalFloor ? minAtoms : notionalFloor, atomScale),
  };
}

/**
 * Resolve a requested target into step-aligned exact quantities for
 * `sliceCount` slices. The target is floored to a whole number of steps and
 * the smallest even slice must clear the per-slice floor.
 */
export function quantizeTwapTarget(input: {
  readonly metadata: TwapMarketMetadata;
  readonly target: TwapTarget;
  readonly sliceCount: number;
}): TwapQuantizeResult {
  const { metadata, target } = input;
  if (!Number.isSafeInteger(input.sliceCount) || input.sliceCount <= 0) {
    throw new Error('Market TWAP slice count must be a positive integer');
  }
  const constraints = deriveTwapQuantityConstraints(metadata);
  if (isTwapQuantizeRejection(constraints)) return constraints;

  const { atomScale } = constraints;
  const stepAtoms = BigInt(constraints.quantityStep.atoms);
  const targetSteps = targetStepCount(metadata, target, stepAtoms, atomScale);
  if (isTwapQuantizeRejection(targetSteps)) return targetSteps;

  const sliceCount = BigInt(input.sliceCount);
  const smallestSteps = targetSteps / sliceCount;
  const largestSteps = smallestSteps + (targetSteps % sliceCount === 0n ? 0n : 1n);
  if (
    constraints.maxQuantity !== undefined &&
    compareScaled(
      largestSteps * stepAtoms,
      atomScale,
      BigInt(constraints.maxQuantity.atoms),
      constraints.maxQuantity.atomScale,
    ) > 0
  ) {
    return {
      kind: 'rejected',
      code: 'target-above-max-slice',
      reason: `target resolves to ${exactQuantityFromAtoms(largestSteps * stepAtoms, atomScale).decimal} for its largest slice, above the ${constraints.maxQuantity.decimal} ${metadata.symbol} maximum`,
    };
  }
  const minSliceAtoms = BigInt(constraints.minSliceQuantity.atoms);
  if (smallestSteps * stepAtoms < minSliceAtoms) {
    return {
      kind: 'rejected',
      code: 'target-below-min-slice',
      reason: `target resolves to ${exactQuantityFromAtoms(smallestSteps * stepAtoms, atomScale).decimal} per slice, below the ${constraints.minSliceQuantity.decimal} ${metadata.symbol} floor`,
    };
  }

  return {
    kind: 'quantized',
    atomScale,
    sliceCount: input.sliceCount,
    target: exactQuantityFromAtoms(targetSteps * stepAtoms, atomScale),
    quantityStep: constraints.quantityStep,
    minQuantity: constraints.minQuantity,
    ...(constraints.maxQuantity === undefined ? {} : { maxQuantity: constraints.maxQuantity }),
    contractSize: constraints.contractSize,
    minSliceQuantity: constraints.minSliceQuantity,
    smallestSliceQuantity: exactQuantityFromAtoms(smallestSteps * stepAtoms, atomScale),
  };
}

export function isTwapQuantizeRejection(value: unknown): value is TwapQuantizeRejection {
  return typeof value === 'object' && value !== null && (value as { kind?: unknown }).kind === 'rejected';
}

/** Whole steps the target resolves to, floored. */
function targetStepCount(
  metadata: TwapMarketMetadata,
  target: TwapTarget,
  stepAtoms: bigint,
  atomScale: number,
): bigint | TwapQuantizeRejection {
  if (target.kind === 'base') {
    const requested = readDecimal(target.quantity);
    if (requested === undefined) return missing('TWAP target quantity is not an unsigned decimal');
    const scale = Math.max(atomScale, requested.atomScale);
    const requestedAtoms = rescale(requested.atoms, requested.atomScale, scale);
    const scaledStep = rescale(stepAtoms, atomScale, scale);
    return requestedAtoms / scaledStep;
  }

  const notional = readDecimal(target.quoteNotional);
  if (notional === undefined) return missing('TWAP target notional is not an unsigned decimal');
  const mark = readDecimal(metadata.markPrice);
  if (mark === undefined || mark.atoms <= 0n) {
    return missing(`${metadata.symbol} mark price is missing or not a positive decimal`);
  }
  return (
    (notional.atoms * 10n ** BigInt(mark.atomScale + atomScale)) /
    (mark.atoms * stepAtoms * 10n ** BigInt(notional.atomScale))
  );
}

/** Quantity atoms needed to clear the venue min notional, rounded up to a step. */
function notionalFloorAtoms(
  metadata: TwapMarketMetadata,
  stepAtoms: bigint,
  atomScale: number,
): bigint | TwapQuantizeRejection {
  const minNotional = readDecimal(metadata.minNotional);
  if (minNotional === undefined || minNotional.atoms <= 0n) return 0n;
  const mark = readDecimal(metadata.markPrice);
  if (mark === undefined || mark.atoms <= 0n) {
    return missing(`${metadata.symbol} mark price is required to enforce its minimum notional`);
  }
  const steps = divideCeil(
    minNotional.atoms * 10n ** BigInt(mark.atomScale + atomScale),
    mark.atoms * stepAtoms * 10n ** BigInt(minNotional.atomScale),
  );
  return steps * stepAtoms;
}

function multiplyDecimalValues(
  left: number | string | null | undefined,
  right: number | string | null | undefined,
): string | undefined {
  const leftDecimal = readDecimal(left);
  const rightDecimal = readDecimal(right);
  if (leftDecimal === undefined || rightDecimal === undefined) return undefined;
  return exactQuantityFromAtoms(
    leftDecimal.atoms * rightDecimal.atoms,
    leftDecimal.atomScale + rightDecimal.atomScale,
  ).decimal;
}

function finitePositiveDecimalValue(value: number | string | null | undefined): string | undefined {
  const decimal = readDecimal(value);
  if (decimal === undefined || decimal.atoms <= 0n) return undefined;
  return exactQuantityFromAtoms(decimal.atoms, decimal.atomScale).decimal;
}

function readDecimal(value: number | string | null | undefined): { atoms: bigint; atomScale: number } | undefined {
  if (value === undefined || value === null) return undefined;
  const text = typeof value === 'number' ? decimalStringFromNumber(value) : value.trim();
  if (text === undefined) return undefined;
  return parseUnsignedDecimal(text);
}

/** Plain (never exponential) decimal text for a finite non-negative number. */
function decimalStringFromNumber(value: number): string | undefined {
  if (!Number.isFinite(value) || value < 0) return undefined;
  const text = value.toString();
  if (!text.includes('e') && !text.includes('E')) return text;
  const [mantissa, exponentText] = text.split(/e/i);
  const exponent = Number(exponentText);
  if (!Number.isFinite(exponent)) return undefined;
  const [whole, fraction = ''] = mantissa.split('.');
  const digits = `${whole}${fraction}`;
  const pointIndex = whole.length + exponent;
  if (pointIndex <= 0) return `0.${'0'.repeat(-pointIndex)}${digits}`;
  if (pointIndex >= digits.length) return `${digits}${'0'.repeat(pointIndex - digits.length)}`;
  return `${digits.slice(0, pointIndex)}.${digits.slice(pointIndex)}`;
}

function rescale(atoms: bigint, fromScale: number, toScale: number): bigint {
  return toScale === fromScale ? atoms : atoms * 10n ** BigInt(toScale - fromScale);
}

function compareScaled(left: bigint, leftScale: number, right: bigint, rightScale: number): number {
  const scale = Math.max(leftScale, rightScale);
  const normalizedLeft = left * 10n ** BigInt(scale - leftScale);
  const normalizedRight = right * 10n ** BigInt(scale - rightScale);
  if (normalizedLeft === normalizedRight) return 0;
  return normalizedLeft < normalizedRight ? -1 : 1;
}

function divideCeil(numerator: bigint, denominator: bigint): bigint {
  return numerator % denominator === 0n ? numerator / denominator : numerator / denominator + 1n;
}

function missing(reason: string): TwapQuantizeRejection {
  return { kind: 'rejected', code: 'market-metadata-missing', reason };
}
