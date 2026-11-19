import type { ExchangeName } from './types';

/** Capabilities of this protocol slice, separate from transport/ledger readiness. */
export interface DelegatedActivationScopeV1 {
  place?: boolean;
  close?: 'reduceOnly' | 'unsupported';
  amend?: boolean;
  cancel?: boolean;
  cancelAll?: boolean;
  nuke?: boolean;
}

export interface DelegatedActivationCapabilities {
  delegatedOrderFramesV1: boolean;
  delegatedOrderLedgerV1?: boolean;
  delegatedOrderActionsV1?: boolean;
  delegatedOrderActionLedgerV1?: boolean;
  delegatedActivationScopeV1?: DelegatedActivationScopeV1;
  /** Venue echoed by fresh box capability discovery for this joined account. */
  delegatedVenueV1?: string | null;
}

export interface DelegatedActivationFlags {
  /** Resolved by the host for this owner; never a signer-selection fallback. */
  DELEGATED: boolean;
}

type RefusalCode =
  | 'delegatedDisabled'
  | 'delegatedVenueUnavailable'
  | 'delegatedOrderFramesUnavailable'
  | 'delegatedOrderLedgerUnavailable'
  | 'delegatedPlaceUnavailable'
  | 'delegatedCloseUnavailable'
  | 'delegatedConnectionNotAdmitted'
  | 'delegatedActionUnavailable'
  | 'delegatedReduceOnlyRequired';

export class DelegatedActivationError extends Error {
  constructor(
    readonly code: RefusalCode,
    message: string,
  ) {
    super(message);
    this.name =
      code === 'delegatedActionUnavailable'
        ? 'DelegatedActionUnavailable'
        : code === 'delegatedReduceOnlyRequired'
          ? 'DelegatedReduceOnlyRequired'
          : 'DelegatedActivationUnavailable';
  }
}

export function requireDelegatedActivation(
  capabilities: DelegatedActivationCapabilities,
  flags: DelegatedActivationFlags,
  venue: ExchangeName,
): void {
  if (flags?.DELEGATED !== true)
    throw new DelegatedActivationError('delegatedDisabled', 'Server trading isn’t enabled for this account');
  if (capabilities.delegatedOrderFramesV1 !== true)
    throw new DelegatedActivationError(
      'delegatedOrderFramesUnavailable',
      'This server doesn’t support order submissions yet',
    );
  if (capabilities.delegatedOrderLedgerV1 !== true)
    throw new DelegatedActivationError(
      'delegatedOrderLedgerUnavailable',
      'This connection doesn’t support confirmed order submissions yet',
    );
  if (capabilities.delegatedVenueV1 !== venue)
    throw new DelegatedActivationError('delegatedVenueUnavailable', 'This connection is configured for a different exchange');
  if (capabilities.delegatedActivationScopeV1?.place !== true)
    throw new DelegatedActivationError('delegatedPlaceUnavailable', 'Order placement isn’t available on this connection');
  if (capabilities.delegatedActivationScopeV1.close !== 'reduceOnly')
    throw new DelegatedActivationError(
      'delegatedCloseUnavailable',
      'Closing positions isn’t available on this connection',
    );
  // Amend, venue cancel, cancel-all and nuke deliberately do not gate activation.
}

export function requireDelegatedAction(action: string, params: Record<string, unknown>): void {
  if (action === 'place') return;
  if (action === 'close') {
    if (params?.reduceOnly !== true)
      throw new DelegatedActivationError('delegatedReduceOnlyRequired', 'Closing a position requires a reduce-only order');
    return;
  }
  throw new DelegatedActivationError(
    'delegatedActionUnavailable',
    `${action} isn’t available here; only placing orders and reduce-only closing are supported`,
  );
}
