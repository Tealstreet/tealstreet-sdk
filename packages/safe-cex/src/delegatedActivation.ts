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
}

export interface DelegatedActivationFlags {
  /** Resolved by the host for this owner; never a signer-selection fallback. */
  DELEGATED_BYBIT: boolean;
}

type RefusalCode =
  | 'delegatedBybitDisabled'
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
): void {
  if (flags?.DELEGATED_BYBIT !== true)
    throw new DelegatedActivationError('delegatedBybitDisabled', 'DELEGATED_BYBIT is not enabled for this owner');
  if (capabilities.delegatedOrderFramesV1 !== true)
    throw new DelegatedActivationError(
      'delegatedOrderFramesUnavailable',
      'DELEGATED order frames are not enabled on this box',
    );
  if (capabilities.delegatedOrderLedgerV1 !== true)
    throw new DelegatedActivationError(
      'delegatedOrderLedgerUnavailable',
      'DELEGATED requires the router ledger on this connection',
    );
  if (capabilities.delegatedActivationScopeV1?.place !== true)
    throw new DelegatedActivationError('delegatedPlaceUnavailable', 'DELEGATED place capability is unavailable');
  if (capabilities.delegatedActivationScopeV1.close !== 'reduceOnly')
    throw new DelegatedActivationError(
      'delegatedCloseUnavailable',
      'DELEGATED reduce-only close capability is unavailable',
    );
  // Amend, venue cancel, cancel-all and nuke deliberately do not gate activation.
}

export function requireDelegatedAction(action: string, params: Record<string, unknown>): void {
  if (action === 'place') return;
  if (action === 'close') {
    if (params?.reduceOnly !== true)
      throw new DelegatedActivationError('delegatedReduceOnlyRequired', 'DELEGATED close requires reduceOnly=true');
    return;
  }
  throw new DelegatedActivationError(
    'delegatedActionUnavailable',
    `DELEGATED ${action} is unavailable; only place and reduce-only close are enabled`,
  );
}
