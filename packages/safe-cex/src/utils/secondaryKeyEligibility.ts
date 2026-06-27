import { ExchangeName } from '../types';
import { exchangeSupportsSecondaryKey } from './exchangeSupportsSecondaryKey';
import { isDirectTransport, resolveAccountTransport, TransportAccount, TransportMetadata } from './transportResolution';

// A secondary read-only key only provides value when the exchange supports one AND
// the primary's resolved transport is NOT Direct: a Direct primary already egresses
// from the user's own (unwhitelisted) machine, so a second unwhitelisted read key
// buys nothing — reads only benefit when the primary is on a Tealstreet path
// (tealstreetProxy / tealstreetTunnel / requiresCors / enforceWsTunnel). Wallet/
// private-key and Orderly-ed25519 venues have no coherent second read key.
//
// Single source of truth shared by the account form (visibility), the secondary-key
// migration banner, the new-account enforcement gate, and the phase-2 block.
// Independent of ENABLE_AWS_PROXY_MIGRATION — that flag gates UI visibility, not
// whether a key would actually help.
export const isEligibleForSecondaryKey = (
  exchange: ExchangeName,
  account: TransportAccount,
  metadata: TransportMetadata,
): boolean => exchangeSupportsSecondaryKey(exchange) && !isDirectTransport(resolveAccountTransport(account, metadata));
