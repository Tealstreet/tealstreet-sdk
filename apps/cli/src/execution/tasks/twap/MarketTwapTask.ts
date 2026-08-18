import type { Market, OrderSide } from '@tealstreet/safe-cex/src/types';

import type {
  ConfirmedOrders,
  ConfirmedOrderHandle,
  ExactQuantity,
  LastGoodOrderTruth,
  OrderTruth,
} from '../../confirmed';
import { baseQuantityToContracts, contractsToBaseQuantity } from './contractQuantity';
import { scheduleMarketTwap, type MarketTwapSlice } from './schedule';

export type MarketTwapLifecycle = 'created' | 'running' | 'paused' | 'stopping' | 'stopped' | 'completed' | 'partial';

export type MarketTwapSpec = {
  readonly taskId: string;
  readonly market: Market;
  readonly side: OrderSide;
  readonly reduceOnly?: boolean;
  /** Base-denominated target and constraints. */
  readonly target: ExactQuantity;
  readonly quantityStep: ExactQuantity;
  readonly minQuantity: ExactQuantity;
  readonly maxQuantity?: ExactQuantity;
  /** Base units per venue-native placement unit. */
  readonly contractSize: ExactQuantity;
  readonly sliceCount: number;
  readonly startAtMs: number;
  readonly durationMs: number;
};

export type MarketTwapReport = {
  readonly taskId: string;
  readonly symbol: string;
  readonly side: OrderSide;
  readonly lifecycle: MarketTwapLifecycle;
  readonly confirmedFilled: ExactQuantity;
  readonly reservedUnresolved: ExactQuantity;
  readonly rejected: ExactQuantity;
  readonly residual: ExactQuantity;
  readonly slices: readonly SliceReport[];
};

export type SliceReport = {
  readonly unitId: string;
  readonly scheduledAtMs: number;
  readonly requested: ExactQuantity;
  /** Immutable report snapshot; handle and observation quantities are base-denominated. */
  readonly truth: OrderTruth | undefined;
  readonly invalidReason?: string;
};

type EnteredSlice = {
  readonly handle: ConfirmedOrderHandle;
  readonly baseQuantity: ExactQuantity;
};

export class MarketTwapTask {
  readonly spec: MarketTwapSpec;
  private readonly orders: ConfirmedOrders;
  private readonly slices: readonly MarketTwapSlice[];
  private readonly entered = new Map<string, EnteredSlice>();
  private readonly placementPending = new Set<string>();
  private lifecycle: MarketTwapLifecycle = 'created';
  private fenced = false;

  constructor(input: { readonly spec: MarketTwapSpec; readonly orders: ConfirmedOrders }) {
    this.spec = input.spec;
    this.orders = input.orders;
    this.slices = scheduleMarketTwap({
      target: input.spec.target,
      sliceCount: input.spec.sliceCount,
      startAtMs: input.spec.startAtMs,
      durationMs: input.spec.durationMs,
      constraints: {
        quantityStep: input.spec.quantityStep,
        minQuantity: input.spec.minQuantity,
        maxQuantity: input.spec.maxQuantity,
        contractSize: input.spec.contractSize,
      },
    });
  }

  start(): void {
    if (this.lifecycle === 'created') this.lifecycle = 'running';
  }

  pause(): void {
    if (this.lifecycle === 'running') this.lifecycle = 'paused';
  }

  stop(): void {
    this.fenced = true;
    this.lifecycle = this.hasUnresolved() ? 'stopping' : 'stopped';
  }

  hasScheduledWork(): boolean {
    if (this.fenced || this.lifecycle !== 'running') return false;
    return this.slices.some(
      (slice) =>
        !('invalidReason' in slice) &&
        !this.entered.has(slice.logicalUnitId) &&
        !this.placementPending.has(slice.logicalUnitId),
    );
  }

  hasDueWork(nowMs: number): boolean {
    if (!this.hasScheduledWork()) return false;
    return this.slices.some(
      (slice) =>
        !('invalidReason' in slice) &&
        !this.entered.has(slice.logicalUnitId) &&
        !this.placementPending.has(slice.logicalUnitId) &&
        slice.scheduledAtMs <= nowMs,
    );
  }

  nextScheduledAt(): number | undefined {
    if (!this.hasScheduledWork()) return undefined;
    let next: number | undefined;
    for (const slice of this.slices) {
      if (
        'invalidReason' in slice ||
        this.entered.has(slice.logicalUnitId) ||
        this.placementPending.has(slice.logicalUnitId)
      ) {
        continue;
      }
      if (next === undefined || slice.scheduledAtMs < next) next = slice.scheduledAtMs;
    }
    return next;
  }

  hasUnresolved(): boolean {
    if (this.placementPending.size > 0) return true;
    for (const entered of this.entered.values()) {
      const truth = this.orders.truth(entered.handle);
      if (truth.kind === 'unconfirmed' || truth.kind === 'open' || truth.kind === 'incident') return true;
    }
    return false;
  }

  async placeDueSlices(nowMs: number): Promise<void> {
    if (!this.hasDueWork(nowMs)) return;
    for (const slice of this.slices) {
      if (
        'invalidReason' in slice ||
        this.entered.has(slice.logicalUnitId) ||
        this.placementPending.has(slice.logicalUnitId) ||
        slice.scheduledAtMs > nowMs ||
        this.fenced ||
        this.lifecycle !== 'running'
      ) {
        continue;
      }
      const nativeQuantity = baseQuantityToContracts(slice.quantity, this.spec.contractSize);
      this.placementPending.add(slice.logicalUnitId);
      try {
        const handle = await this.orders.place({
          identity: { ownerId: this.spec.taskId, unitId: slice.logicalUnitId },
          order: { market: this.spec.market, side: this.spec.side, reduceOnly: this.spec.reduceOnly },
          exact: nativeQuantity,
        });
        this.entered.set(slice.logicalUnitId, { handle, baseQuantity: slice.quantity });
        if (this.orders.truth(handle).kind === 'incident') {
          this.pause();
          return;
        }
      } finally {
        this.placementPending.delete(slice.logicalUnitId);
        this.refreshLifecycle();
      }
    }
    this.refreshLifecycle();
  }

  async refreshHandle(handle: ConfirmedOrderHandle): Promise<OrderTruth> {
    const entered = this.entered.get(handle.identity.unitId);
    if (entered?.handle !== handle) throw new Error('Handle does not belong to this confirmed TWAP');
    const truth = await this.orders.lookup(handle);
    if (truth.kind === 'incident') this.pause();
    this.refreshLifecycle();
    return truth;
  }

  unresolvedHandles(): readonly ConfirmedOrderHandle[] {
    const result: ConfirmedOrderHandle[] = [];
    for (const entered of this.entered.values()) {
      const truth = this.orders.truth(entered.handle);
      if (truth.kind === 'unconfirmed' || truth.kind === 'open' || truth.kind === 'incident') {
        result.push(entered.handle);
      }
    }
    return result;
  }

  isSettled(): boolean {
    return !this.hasScheduledWork() && !this.hasUnresolved();
  }

  report(): MarketTwapReport {
    let filledAtoms = 0n;
    let reservedAtoms = 0n;
    let rejectedAtoms = 0n;
    const scale = this.spec.target.atomScale;

    const addProjection = (
      truth: Extract<LastGoodOrderTruth, { kind: 'open' | 'terminal' }>,
      status: 'ordinary' | 'incident',
    ): void => {
      const filled = baseAtScale(truth.observation.filled, this.spec.contractSize, scale);
      const residual = baseAtScale(truth.observation.residual, this.spec.contractSize, scale);
      filledAtoms += filled;
      if (truth.kind === 'open' || status === 'incident') reservedAtoms += residual;
      if (
        truth.kind === 'terminal' &&
        (truth.observation.status === 'rejected' || truth.observation.status === 'not_placed')
      ) {
        rejectedAtoms += residual;
      }
    };

    const sliceReports: SliceReport[] = this.slices.map((slice) => {
      const entered = this.entered.get(slice.logicalUnitId);
      const truth = entered === undefined ? undefined : this.orders.truth(entered.handle);
      const reportTruth = truth === undefined ? undefined : projectTruthToBase(truth, this.spec.contractSize);
      if ('invalidReason' in slice) {
        rejectedAtoms += BigInt(slice.requested.atoms);
        return {
          unitId: slice.logicalUnitId,
          scheduledAtMs: slice.scheduledAtMs,
          requested: slice.requested,
          truth: reportTruth,
          invalidReason: slice.invalidReason,
        };
      }

      if (this.placementPending.has(slice.logicalUnitId)) {
        reservedAtoms += BigInt(slice.quantity.atoms);
      } else if (truth?.kind === 'open' || truth?.kind === 'terminal') {
        addProjection(truth, 'ordinary');
      } else if (truth?.kind === 'incident') {
        if (truth.lastGood.kind === 'open' || truth.lastGood.kind === 'terminal') {
          addProjection(truth.lastGood, 'incident');
        } else {
          reservedAtoms += BigInt(slice.quantity.atoms);
        }
      } else if (entered !== undefined) {
        reservedAtoms += BigInt(slice.quantity.atoms);
      }

      return {
        unitId: slice.logicalUnitId,
        scheduledAtMs: slice.scheduledAtMs,
        requested: slice.quantity,
        truth: reportTruth,
      };
    });

    const targetAtoms = BigInt(this.spec.target.atoms);
    const residualAtoms = targetAtoms - filledAtoms - reservedAtoms;
    return {
      taskId: this.spec.taskId,
      symbol: this.spec.market.symbol,
      side: this.spec.side,
      lifecycle: this.lifecycle,
      confirmedFilled: atomsToExact(filledAtoms, scale),
      reservedUnresolved: atomsToExact(reservedAtoms, scale),
      rejected: atomsToExact(rejectedAtoms, scale),
      residual: atomsToExact(residualAtoms < 0n ? 0n : residualAtoms, scale),
      slices: sliceReports,
    };
  }

  private refreshLifecycle(): void {
    if (this.lifecycle === 'paused') return;
    if (this.fenced) {
      this.lifecycle = this.hasUnresolved() ? 'stopping' : 'stopped';
      return;
    }
    if (!this.hasScheduledWork() && !this.hasUnresolved()) {
      this.lifecycle = this.report().residual.atoms === '0' ? 'completed' : 'partial';
    }
  }
}

const baseAtScale = (native: ExactQuantity, contractSize: ExactQuantity, targetScale: number): bigint => {
  const base = contractsToBaseQuantity(native, contractSize);
  if (base.atomScale === targetScale) return BigInt(base.atoms);
  if (base.atomScale < targetScale) {
    return BigInt(base.atoms) * 10n ** BigInt(targetScale - base.atomScale);
  }
  const divisor = 10n ** BigInt(base.atomScale - targetScale);
  const atoms = BigInt(base.atoms);
  if (atoms % divisor !== 0n) throw new Error('Observed fill cannot be represented at the TWAP target scale');
  return atoms / divisor;
};

const projectTruthToBase = (truth: OrderTruth, contractSize: ExactQuantity): OrderTruth => {
  const handle = {
    ...truth.handle,
    exact: contractsToBaseQuantity(truth.handle.exact, contractSize),
  };
  if (truth.kind === 'incident') {
    return {
      ...truth,
      handle,
      lastGood: projectLastGoodToBase(truth.lastGood, contractSize),
    };
  }
  return projectLastGoodToBase(truth, contractSize, handle);
};

const projectLastGoodToBase = (
  truth: LastGoodOrderTruth,
  contractSize: ExactQuantity,
  projectedHandle: ConfirmedOrderHandle = {
    ...truth.handle,
    exact: contractsToBaseQuantity(truth.handle.exact, contractSize),
  },
): LastGoodOrderTruth => {
  if (truth.kind === 'unconfirmed') return { ...truth, handle: projectedHandle };
  if (truth.kind === 'open') {
    return {
      ...truth,
      handle: projectedHandle,
      observation: {
        ...truth.observation,
        total: contractsToBaseQuantity(truth.observation.total, contractSize),
        filled: contractsToBaseQuantity(truth.observation.filled, contractSize),
        residual: contractsToBaseQuantity(truth.observation.residual, contractSize),
      },
    };
  }
  return {
    ...truth,
    handle: projectedHandle,
    observation: {
      ...truth.observation,
      total: contractsToBaseQuantity(truth.observation.total, contractSize),
      filled: contractsToBaseQuantity(truth.observation.filled, contractSize),
      residual: contractsToBaseQuantity(truth.observation.residual, contractSize),
    },
  };
};

const atomsToExact = (atoms: bigint, atomScale: number): ExactQuantity => {
  const digits = atoms.toString();
  if (atomScale === 0) return { decimal: digits, atoms: digits, atomScale };
  const padded = digits.padStart(atomScale + 1, '0');
  const split = padded.length - atomScale;
  return { decimal: `${padded.slice(0, split)}.${padded.slice(split)}`, atoms: digits, atomScale };
};
