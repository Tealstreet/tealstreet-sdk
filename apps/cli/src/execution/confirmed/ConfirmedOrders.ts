import type { Exchange } from '@tealstreet/safe-cex/src/exchanges/base';
import { trackedRegularOrderCapabilityFor } from '@tealstreet/safe-cex/src/tracked';
import type { TrackedLookupResult, TrackedRegularOrderCapability } from '@tealstreet/safe-cex/src/tracked';
import { OrderType } from '@tealstreet/safe-cex/src/types';

import { deriveCallerId } from './deriveCallerId';
import type {
  ConfirmedAuditSink,
  ConfirmedMarketOrder,
  ConfirmedOrderHandle,
  ExactQuantity,
  LastGoodOrderTruth,
  LogicalOrderIdentity,
  OpenOrderTruthProjection,
  OrderTruth,
  TerminalOrderTruthProjection,
} from './types';

export type ConfirmedOrders = {
  place(input: {
    readonly identity: LogicalOrderIdentity;
    readonly order: ConfirmedMarketOrder;
    readonly exact: ExactQuantity;
  }): Promise<ConfirmedOrderHandle>;
  lookup(handle: ConfirmedOrderHandle): Promise<OrderTruth>;
  truth(handle: ConfirmedOrderHandle): OrderTruth;
  forget(handle: ConfirmedOrderHandle): void;
};

type BoundOrder = {
  readonly handle: ConfirmedOrderHandle;
  readonly capability: TrackedRegularOrderCapability;
  venueOrderId: string | undefined;
  truth: OrderTruth;
  lookupInFlight: Promise<OrderTruth> | undefined;
};

type Reservation = {
  readonly fingerprint: string;
  pending: Promise<ConfirmedOrderHandle> | undefined;
  bound: BoundOrder | undefined;
};

export const createConfirmedOrders = (opts: {
  readonly exchange: Exchange;
  readonly signerFingerprint: string;
  readonly audit?: ConfirmedAuditSink;
}): ConfirmedOrders => {
  const byIdentity = new Map<string, Reservation>();
  const byCallerId = new Map<string, string>();

  const identityKey = (identity: LogicalOrderIdentity): string => `${identity.ownerId}\0${identity.unitId}`;

  const place = (input: {
    readonly identity: LogicalOrderIdentity;
    readonly order: ConfirmedMarketOrder;
    readonly exact: ExactQuantity;
  }): Promise<ConfirmedOrderHandle> => {
    const capability = trackedRegularOrderCapabilityFor(opts.exchange, input.order.market);
    if (capability === undefined) {
      throw new Error(`Confirmed execution is not available for ${opts.exchange.name} ${input.order.market.symbol}`);
    }

    const identity = Object.freeze({ ownerId: input.identity.ownerId, unitId: input.identity.unitId });
    const exact = assertExactQuantity(input.exact);
    const amount = Number(exact.decimal);
    const marketId = input.order.market.id;
    const symbol = input.order.market.symbol;
    const side = input.order.side;
    const reduceOnly = input.order.reduceOnly === true;
    const fingerprint = canonicalFingerprint({ marketId, symbol, side, reduceOnly, exact });
    const key = identityKey(identity);

    const existing = byIdentity.get(key);
    if (existing !== undefined) {
      if (existing.fingerprint !== fingerprint) {
        throw new Error(`Logical identity ${key} is already bound to a different order fingerprint`);
      }
      if (existing.bound !== undefined) return Promise.resolve(existing.bound.handle);
      if (existing.pending !== undefined) return existing.pending;
      throw new Error(`Logical identity ${key} has an invalid reservation`);
    }

    const callerId = deriveCallerId({
      venue: opts.exchange.name,
      signerFingerprint: opts.signerFingerprint,
      ownerId: identity.ownerId,
      unitId: identity.unitId,
    });
    const boundIdentity = byCallerId.get(callerId);
    if (boundIdentity !== undefined && boundIdentity !== key) {
      throw new Error(`Caller ID ${callerId} collides with a distinct logical identity`);
    }
    const wireClientOrderId = capability.composeClientOrderId(callerId);

    const reservation: Reservation = { fingerprint, pending: undefined, bound: undefined };
    byIdentity.set(key, reservation);
    byCallerId.set(callerId, key);

    const deferred = createDeferred<ConfirmedOrderHandle>();
    reservation.pending = deferred.promise;
    void placeOnce({
      capability,
      identity,
      exact,
      amount,
      fingerprint,
      marketId,
      symbol,
      side,
      reduceOnly,
      callerId,
      wireClientOrderId,
      audit: opts.audit,
      exchange: opts.exchange,
    }).then(
      (bound) => {
        reservation.bound = bound;
        deferred.resolve(bound.handle);
      },
      deferred.reject,
    );
    return deferred.promise;
  };

  const boundFor = (handle: ConfirmedOrderHandle): BoundOrder => {
    const bound = byIdentity.get(identityKey(handle.identity))?.bound;
    if (bound === undefined || bound.handle !== handle) {
      throw new Error('Unknown confirmed-order handle');
    }
    return bound;
  };

  return {
    place,
    lookup(handle) {
      const bound = boundFor(handle);
      if (bound.truth.kind === 'incident') return Promise.resolve(bound.truth);
      if (bound.lookupInFlight !== undefined) return bound.lookupInFlight;

      const lookup = bound.capability
        .lookupExact({ wireClientOrderId: bound.handle.wireClientOrderId })
        .catch(
          (error: unknown): TrackedLookupResult => ({
            kind: 'indeterminate',
            reason: 'lookup-threw',
            raw: error,
          }),
        )
        .then((observed) => {
          bound.truth = applyLookup(bound, observed);
          return bound.truth;
        });
      bound.lookupInFlight = lookup;
      void lookup.finally(() => {
        if (bound.lookupInFlight === lookup) bound.lookupInFlight = undefined;
      });
      return lookup;
    },
    truth(handle) {
      return boundFor(handle).truth;
    },
    forget(handle) {
      const bound = boundFor(handle);
      if (bound.truth.kind !== 'terminal') {
        throw new Error(`Cannot forget confirmed order while truth is ${bound.truth.kind}`);
      }
      if (bound.lookupInFlight !== undefined) {
        throw new Error('Cannot forget confirmed order while an exact lookup is in flight');
      }

      const key = identityKey(handle.identity);
      if (byCallerId.get(handle.callerId) !== key) {
        throw new Error('Confirmed-order caller identity is inconsistent');
      }
      byIdentity.delete(key);
      byCallerId.delete(handle.callerId);
    },
  };
};

const placeOnce = async (input: {
  readonly capability: TrackedRegularOrderCapability;
  readonly identity: LogicalOrderIdentity;
  readonly exact: ExactQuantity;
  readonly amount: number;
  readonly fingerprint: string;
  readonly marketId: string;
  readonly symbol: string;
  readonly side: ConfirmedMarketOrder['side'];
  readonly reduceOnly: boolean;
  readonly callerId: string;
  readonly wireClientOrderId: string;
  readonly audit: ConfirmedAuditSink | undefined;
  readonly exchange: Exchange;
}): Promise<BoundOrder> => {
  input.audit?.emit({
    kind: 'confirmed-place',
    ownerId: input.identity.ownerId,
    unitId: input.identity.unitId,
    callerId: input.callerId,
    wireClientOrderId: input.wireClientOrderId,
    marketId: input.marketId,
    symbol: input.symbol,
    side: input.side,
    reduceOnly: input.reduceOnly,
    exact: input.exact,
  });

  let resolved: { kind: 'resolved'; value: unknown } | { kind: 'thrown'; error: unknown };
  try {
    const value = await input.exchange.placeOrder({
      symbol: input.symbol,
      type: OrderType.Market,
      side: input.side,
      reduceOnly: input.reduceOnly,
      amount: input.amount,
      clientOrderId: input.callerId,
    });
    resolved = { kind: 'resolved', value };
  } catch (error) {
    resolved = { kind: 'thrown', error };
  }

  const placement = input.capability.classifyPlacement(resolved);
  const venueOrderId = placement.kind === 'accepted' ? placement.venueOrderId : undefined;
  input.audit?.emit({
    kind: 'confirmed-placed',
    ownerId: input.identity.ownerId,
    unitId: input.identity.unitId,
    callerId: input.callerId,
    wireClientOrderId: input.wireClientOrderId,
    placement,
    ...(venueOrderId === undefined ? {} : { venueOrderId }),
  });

  const handle = Object.freeze({
    identity: input.identity,
    fingerprint: input.fingerprint,
    marketId: input.marketId,
    symbol: input.symbol,
    callerId: input.callerId,
    wireClientOrderId: input.wireClientOrderId,
    exact: input.exact,
    placement,
    ...(venueOrderId === undefined ? {} : { venueOrderId }),
  });
  const truth: OrderTruth =
    placement.kind === 'rejected' &&
    'identityConflict' in placement &&
    placement.identityConflict === 'duplicate-client-order-id'
      ? { kind: 'unconfirmed', handle, lastIndeterminateReason: 'duplicate-client-order-id' }
      : placement.kind === 'rejected'
        ? {
            kind: 'terminal',
            handle,
            observation: zeroFillProjection('rejected', input.exact),
          }
        : { kind: 'unconfirmed', handle };

  return { handle, capability: input.capability, venueOrderId, truth, lookupInFlight: undefined };
};

const applyLookup = (bound: BoundOrder, observed: TrackedLookupResult): OrderTruth => {
  const current = bound.truth;
  if (current.kind === 'incident') return current;

  if (observed.kind === 'indeterminate') {
    if (current.kind === 'unconfirmed') return { ...current, lastIndeterminateReason: observed.reason };
    if (current.kind === 'open') {
      return { ...current, observation: { ...current.observation, lastIndeterminateReason: observed.reason } };
    }
    return { ...current, observation: { ...current.observation, lastIndeterminateReason: observed.reason } };
  }

  if (observed.kind === 'confirmed_absent') {
    const next: OrderTruth = {
      kind: 'terminal',
      handle: bound.handle,
      observation: zeroFillProjection('not_placed', bound.handle.exact, observed.raw),
    };
    return transition(current, next, observed.raw);
  }

  if (observed.wireClientOrderId !== bound.handle.wireClientOrderId) {
    return latch(current, 'identity-mismatch', observed.raw);
  }
  if (bound.venueOrderId !== undefined && observed.venueOrderId !== bound.venueOrderId) {
    return latch(current, 'venue-order-id-changed', observed.raw);
  }

  const total = parseExact(observed.totalAmount, bound.handle.exact.atomScale);
  const filled = parseExact(observed.cumulativeFilledAmount, bound.handle.exact.atomScale);
  if (total === undefined || filled === undefined) {
    return latch(current, 'quantity-unparseable', observed.raw);
  }
  if (compareExact(total, bound.handle.exact) !== 0) {
    return latch(current, 'total-mismatch', observed.raw);
  }
  if (compareExact(filled, total) > 0) {
    return latch(current, 'overfill', observed.raw);
  }
  if (observed.status === 'filled' && compareExact(filled, total) !== 0) {
    return latch(current, 'filled-below-total', observed.raw);
  }

  const residual = subtractExact(total, filled);
  const next: OrderTruth =
    observed.status === 'open' || observed.status === 'partially_filled'
      ? {
          kind: 'open',
          handle: bound.handle,
          observation: {
            status: observed.status,
            venueOrderId: observed.venueOrderId,
            total,
            filled,
            residual,
            ...(observed.averagePrice === undefined ? {} : { averagePrice: observed.averagePrice }),
            raw: observed.raw,
          },
        }
      : {
          kind: 'terminal',
          handle: bound.handle,
          observation: {
            status: observed.status,
            venueOrderId: observed.venueOrderId,
            total,
            filled,
            residual,
            ...(observed.averagePrice === undefined ? {} : { averagePrice: observed.averagePrice }),
            raw: observed.raw,
          },
        };

  const transitioned = transition(current, next, observed.raw);
  if (transitioned.kind !== 'incident') {
    bound.venueOrderId = observed.venueOrderId;
  }
  return transitioned;
};

const transition = (current: LastGoodOrderTruth, next: OrderTruth, raw: unknown): OrderTruth => {
  if (next.kind === 'incident' || next.kind === 'unconfirmed') {
    return latch(current, 'illegal-transition', raw);
  }
  if (current.kind === 'unconfirmed') return next;
  if (current.kind === 'open') {
    if (compareExact(next.observation.filled, current.observation.filled) < 0) {
      return latch(current, 'fill-decrease', raw);
    }
    return next;
  }
  if (next.kind === 'terminal' && sameTerminalProjection(current.observation, next.observation)) {
    return current;
  }
  return latch(current, 'terminal-regression', raw);
};

const sameTerminalProjection = (
  left: TerminalOrderTruthProjection,
  right: TerminalOrderTruthProjection,
): boolean =>
  left.status === right.status &&
  compareExact(left.total, right.total) === 0 &&
  compareExact(left.filled, right.filled) === 0 &&
  compareExact(left.residual, right.residual) === 0 &&
  left.averagePrice === right.averagePrice;

const latch = (current: LastGoodOrderTruth, reason: string, raw: unknown): OrderTruth => ({
  kind: 'incident',
  handle: current.handle,
  reason,
  offendingRaw: raw,
  lastGood: current,
});

const zeroFillProjection = (
  status: 'rejected' | 'not_placed',
  total: ExactQuantity,
  raw?: unknown,
): TerminalOrderTruthProjection => ({
  status,
  total,
  filled: exactFromAtoms(0n, total.atomScale),
  residual: total,
  ...(raw === undefined ? {} : { raw }),
});

const assertExactQuantity = (value: ExactQuantity): ExactQuantity => {
  if (!Number.isSafeInteger(value.atomScale) || value.atomScale < 0) {
    throw new Error('ExactQuantity.atomScale must be a nonnegative safe integer');
  }
  if (!/^[1-9]\d*$/.test(value.atoms)) {
    throw new Error('ExactQuantity.atoms must be a canonical positive integer');
  }
  const parsed = parseExact(value.decimal, value.atomScale);
  if (parsed === undefined || parsed.atoms !== value.atoms) {
    throw new Error('ExactQuantity decimal, atoms, and atomScale must describe the same positive quantity');
  }
  const numeric = Number(value.decimal);
  if (!Number.isFinite(numeric) || numeric <= 0) {
    throw new Error('ExactQuantity must project to a finite positive number');
  }
  return Object.freeze({ decimal: value.decimal, atoms: value.atoms, atomScale: value.atomScale });
};

const parseExact = (decimal: string, atomScale: number): ExactQuantity | undefined => {
  if (!/^(?:0|[1-9]\d*)(?:\.\d+)?$/.test(decimal)) return undefined;
  const [whole, fraction = ''] = decimal.split('.');
  if (fraction.length > atomScale && /[1-9]/.test(fraction.slice(atomScale))) return undefined;
  const normalizedFraction = fraction.slice(0, atomScale).padEnd(atomScale, '0');
  const atoms = BigInt(`${whole}${normalizedFraction || ''}`).toString();
  return { decimal, atoms, atomScale };
};

const compareExact = (left: ExactQuantity, right: ExactQuantity): number => {
  const scale = Math.max(left.atomScale, right.atomScale);
  const leftAtoms = BigInt(left.atoms) * 10n ** BigInt(scale - left.atomScale);
  const rightAtoms = BigInt(right.atoms) * 10n ** BigInt(scale - right.atomScale);
  if (leftAtoms === rightAtoms) return 0;
  return leftAtoms < rightAtoms ? -1 : 1;
};

const subtractExact = (total: ExactQuantity, filled: ExactQuantity): ExactQuantity => {
  const scale = Math.max(total.atomScale, filled.atomScale);
  const atoms =
    BigInt(total.atoms) * 10n ** BigInt(scale - total.atomScale) -
    BigInt(filled.atoms) * 10n ** BigInt(scale - filled.atomScale);
  return exactFromAtoms(atoms, scale);
};

const exactFromAtoms = (atoms: bigint, atomScale: number): ExactQuantity => {
  const digits = atoms.toString();
  if (atomScale === 0) return { decimal: digits, atoms: digits, atomScale };
  const padded = digits.padStart(atomScale + 1, '0');
  const split = padded.length - atomScale;
  return { decimal: `${padded.slice(0, split)}.${padded.slice(split)}`, atoms: digits, atomScale };
};

const canonicalFingerprint = (input: {
  readonly marketId: string;
  readonly symbol: string;
  readonly side: ConfirmedMarketOrder['side'];
  readonly reduceOnly: boolean;
  readonly exact: ExactQuantity;
}): string =>
  JSON.stringify({
    marketId: input.marketId,
    symbol: input.symbol,
    side: input.side,
    reduceOnly: input.reduceOnly,
    decimal: input.exact.decimal,
    atoms: input.exact.atoms,
    atomScale: input.exact.atomScale,
  });

const createDeferred = <T>(): {
  readonly promise: Promise<T>;
  readonly resolve: (value: T) => void;
  readonly reject: (reason?: unknown) => void;
} => {
  let resolve: (value: T) => void = () => undefined;
  let reject: (reason?: unknown) => void = () => undefined;
  const promise = new Promise<T>((nextResolve, nextReject) => {
    resolve = nextResolve;
    reject = nextReject;
  });
  return { promise, resolve, reject };
};
