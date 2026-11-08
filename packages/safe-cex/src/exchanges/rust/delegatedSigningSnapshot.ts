import type { ExchangeName, ExchangeOptions, PayloadSigner, RemoteSigner } from '../../types';

import { ExchangeError } from '@tealstreet/safe-cex-wasm/host';

import { hasPrimaryKey, resolveEffectivePublicDataOnly } from './localSignerSetup';

type Callbacks = Pick<ExchangeOptions, 'remoteSigner' | 'payloadSigner'>;
type Snapshot = Readonly<{
  venue: ExchangeName; inspected: boolean; wellFormed: boolean;
  values: Readonly<Callbacks>; callbacks?: Readonly<Callbacks>;
}>;
const snapshotKey = Symbol('delegatedSigningSnapshot');
const snapshots = new WeakMap<object, Snapshot>();
const callbackValues = (remoteSigner: RemoteSigner | undefined, payloadSigner: PayloadSigner | undefined): Readonly<Callbacks> =>
  Object.freeze({ ...(remoteSigner === undefined ? {} : { remoteSigner }),
    ...(payloadSigner === undefined ? {} : { payloadSigner }) });
const invalid = (): never => {
  const message = 'Invalid delegated signer setup';
  throw new ExchangeError(message, { message, kind: 'invalidInput', outcome: 'notSent' });
};
const snapshotOf = (options: Callbacks): Snapshot | undefined => {
  if (!(snapshotKey in options)) return undefined;
  const token = options[snapshotKey];
  return token !== null && typeof token === 'object' ? snapshots.get(token) : undefined;
};

// Called only inside the entry guard, after outer fields have become data values.
const captureCallbacks = ({ remoteSigner, payloadSigner }: Callbacks): Readonly<Callbacks> | undefined => {
  let remote: RemoteSigner | undefined;
  if (remoteSigner !== undefined) {
    if (!remoteSigner || (typeof remoteSigner !== 'object' && typeof remoteSigner !== 'function')
      || !('sign' in remoteSigner) || !('isAvailable' in remoteSigner) || !('onAvailabilityChange' in remoteSigner)) return undefined;
    const { sign, isAvailable, onAvailabilityChange, accountName } = remoteSigner;
    if (typeof sign !== 'function' || typeof isAvailable !== 'function' || typeof onAvailabilityChange !== 'function') return undefined;
    remote = Object.freeze({
      sign: (request: Parameters<RemoteSigner['sign']>[0]) => Reflect.apply(sign, remoteSigner, [request]),
      isAvailable: () => Reflect.apply(isAvailable, remoteSigner, []),
      onAvailabilityChange: (callback: Parameters<RemoteSigner['onAvailabilityChange']>[0]) => Reflect.apply(onAvailabilityChange, remoteSigner, [callback]),
      accountName,
    });
  }
  let payload: PayloadSigner | undefined;
  if (payloadSigner !== undefined) {
    if (typeof payloadSigner !== 'function' || !('call' in payloadSigner)) return undefined;
    const call = payloadSigner.call;
    if (typeof call !== 'function') return undefined;
    payload = Object.freeze(function (this: unknown, request: Parameters<PayloadSigner>[0]) {
      return Reflect.apply(call, payloadSigner, [this, request]);
    });
  }
  return callbackValues(remote, payload);
};

/** One guarded projection owns outer fields and methods across asynchronous loading.
 * The token survives internal spreads, then is stripped from retained exchange options.
 * Callbacks bypassed by public admission retain presence without member reads. */
export function snapshotDelegatedSigningOptions(name: ExchangeName, options: ExchangeOptions): ExchangeOptions;
export function snapshotDelegatedSigningOptions(name: ExchangeName, options: Partial<ExchangeOptions>): Partial<ExchangeOptions>;
export function snapshotDelegatedSigningOptions(name: ExchangeName, options: Partial<ExchangeOptions>): Partial<ExchangeOptions> {
  try {
    const previous = snapshotOf(options);
    Reflect.has(options, 'remoteSigner');
    Reflect.has(options, 'payloadSigner');
    const { remoteSigner, payloadSigner, ...rest } = options;
    const values = { ...rest, ...callbackValues(remoteSigner, payloadSigner) };
    const inspect = !resolveEffectivePublicDataOnly(name, values)
      || (values.publicDataOnly !== true && hasPrimaryKey(name, values));
    const reusable = previous?.venue === name && previous.inspected === inspect
      && previous.values.remoteSigner === remoteSigner && previous.values.payloadSigner === payloadSigner;
    const callbacks = reusable ? previous.callbacks : inspect ? captureCallbacks(values) : undefined;
    const snapshot = reusable ? previous : Object.freeze({ venue: name, inspected: inspect,
      wellFormed: !inspect || callbacks !== undefined,
      values: callbacks ?? callbackValues(remoteSigner, payloadSigner), callbacks });
    const token = Object.freeze({});
    snapshots.set(token, snapshot);
    return Object.freeze({ ...rest, ...snapshot.values, [snapshotKey]: token });
  } catch { return invalid(); }
}

export const delegatedCallbacksWellFormed = (options: Callbacks): boolean => {
  try { return snapshotOf(options)?.wellFormed ?? (captureCallbacks(options) !== undefined); }
  catch { return invalid(); }
};

export const capturedDelegatedCallbacks = (options: Callbacks): Readonly<Callbacks> | undefined =>
  snapshotOf(options)?.callbacks;

/** Remove the construction-only token before any generic options are retained. */
export const withoutDelegatedSigningSnapshot = (options: ExchangeOptions): ExchangeOptions => {
  const decorated: ExchangeOptions & { [snapshotKey]?: unknown } = options;
  const { [snapshotKey]: _snapshot, ...safe } = decorated;
  return safe;
};
