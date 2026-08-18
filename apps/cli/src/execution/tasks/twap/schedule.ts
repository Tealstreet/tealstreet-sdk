import type { ExactQuantity } from '../../confirmed/types';

export interface VenueQuantityConstraints {
  readonly quantityStep: ExactQuantity;
  readonly minQuantity: ExactQuantity;
  readonly maxQuantity?: ExactQuantity;
  /** Base units represented by one venue-native contract. Scheduling itself remains base-denominated. */
  readonly contractSize?: ExactQuantity;
}

export interface MarketTwapScheduleInput {
  readonly target: ExactQuantity;
  readonly sliceCount: number;
  readonly startAtMs: number;
  readonly durationMs: number;
  readonly constraints: VenueQuantityConstraints;
}

export type MarketTwapSlice =
  | {
      readonly logicalUnitId: string;
      readonly scheduledAtMs: number;
      readonly requested: ExactQuantity;
      readonly quantity: ExactQuantity;
    }
  | {
      readonly logicalUnitId: string;
      readonly scheduledAtMs: number;
      readonly requested: ExactQuantity;
      readonly invalidReason: string;
    };

export function scheduleMarketTwap(input: MarketTwapScheduleInput): readonly MarketTwapSlice[] {
  const { target, constraints } = input;
  assertPositiveInteger(input.sliceCount, 'sliceCount');
  assertNonnegativeInteger(input.startAtMs, 'startAtMs');
  assertNonnegativeInteger(input.durationMs, 'durationMs');
  if (input.startAtMs > Number.MAX_SAFE_INTEGER - input.durationMs) {
    throw new Error('Market TWAP absolute end time must be a safe integer');
  }
  assertSameScale(target, constraints.quantityStep, 'quantityStep');
  assertSameScale(target, constraints.minQuantity, 'minQuantity');

  const targetAtoms = positiveAtoms(target, 'target');
  const stepAtoms = positiveAtoms(constraints.quantityStep, 'quantityStep');
  const minAtoms = positiveAtoms(constraints.minQuantity, 'minQuantity');
  if (targetAtoms % stepAtoms !== 0n) {
    throw new Error('Market TWAP target quantity must be a multiple of the venue quantity step');
  }

  const totalSteps = targetAtoms / stepAtoms;
  const sliceCount = BigInt(input.sliceCount);
  const baseSteps = totalSteps / sliceCount;
  const extraSteps = totalSteps % sliceCount;

  return Array.from({ length: input.sliceCount }, (_, index): MarketTwapSlice => {
    const steps = baseSteps + (BigInt(index) < extraSteps ? 1n : 0n);
    const atoms = steps * stepAtoms;
    const requested = exactFromAtoms(atoms, target.atomScale);
    const scheduledAtMs =
      input.sliceCount === 1
        ? input.startAtMs
        : input.startAtMs + Number((BigInt(input.durationMs) * BigInt(index)) / BigInt(input.sliceCount - 1));
    const base = { logicalUnitId: `slice-${index}`, scheduledAtMs, requested } as const;
    if (atoms < minAtoms) {
      return {
        ...base,
        invalidReason: `rounded quantity ${requested.decimal} is below venue minimum ${constraints.minQuantity.decimal}`,
      };
    }
    if (
      constraints.maxQuantity !== undefined &&
      compareScaled(atoms, target.atomScale, positiveAtoms(constraints.maxQuantity, 'maxQuantity'), constraints.maxQuantity.atomScale) > 0
    ) {
      return {
        ...base,
        invalidReason: `rounded quantity ${requested.decimal} exceeds venue maximum ${constraints.maxQuantity.decimal}`,
      };
    }
    return { ...base, quantity: requested };
  });
}

function exactFromAtoms(atoms: bigint, atomScale: number): ExactQuantity {
  const digits = atoms.toString();
  if (atomScale === 0) return { atoms: digits, decimal: digits, atomScale };
  const padded = digits.padStart(atomScale + 1, '0');
  const split = padded.length - atomScale;
  return { atoms: digits, decimal: `${padded.slice(0, split)}.${padded.slice(split)}`, atomScale };
}

function positiveAtoms(quantity: ExactQuantity, name: string): bigint {
  if (!/^\d+$/.test(quantity.atoms)) throw new Error(`Market TWAP ${name} atoms must be an unsigned integer`);
  const atoms = BigInt(quantity.atoms);
  if (atoms <= 0n) throw new Error(`Market TWAP ${name} must be positive`);
  return atoms;
}

function assertSameScale(target: ExactQuantity, other: ExactQuantity, name: string): void {
  if (target.atomScale !== other.atomScale) {
    throw new Error(`Market TWAP target and ${name} must use the same atom scale`);
  }
}

function compareScaled(left: bigint, leftScale: number, right: bigint, rightScale: number): number {
  const scale = Math.max(leftScale, rightScale);
  const normalizedLeft = left * 10n ** BigInt(scale - leftScale);
  const normalizedRight = right * 10n ** BigInt(scale - rightScale);
  if (normalizedLeft === normalizedRight) return 0;
  return normalizedLeft < normalizedRight ? -1 : 1;
}

function assertPositiveInteger(value: number, name: string): void {
  if (!Number.isSafeInteger(value) || value <= 0) throw new Error(`Market TWAP ${name} must be a positive integer`);
}

function assertNonnegativeInteger(value: number, name: string): void {
  if (!Number.isSafeInteger(value) || value < 0) throw new Error(`Market TWAP ${name} must be a non-negative integer`);
}
