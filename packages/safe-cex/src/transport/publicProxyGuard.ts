import type { ExchangeName } from '../types';

import { resolveLighterDeploymentProfile } from '../exchanges/lighter/lighter.types';

/**
 * Public egress refuses venue credentials. Lighter alone uses payload signatures
 * and short-lived read tokens on an explicit set of native API operations.
 */
export class SignedPublicProxyRefusal extends Error {
  /** The host's `invalid` preparation code: a definite failure, so the request counts as not sent. */
  readonly code = 'HTTP_PREPARATION_INVALID';
  constructor(exchange: ExchangeName, url: string) {
    super(`[${exchange}] refused: signed request to the public proxy (${new URL(url).pathname})`);
    this.name = 'SignedPublicProxyRefusal';
  }
}

/**
 * A deployment declared to send its REST, signed writes included, through the public
 * proxy with authorization inside the payload (lighter.types `usesRestProxy`). The one
 * predicate both routes such a venue there and exempts its payload-signed requests.
 */
export const payloadSignedViaPublicProxy = (exchange: string, options: { testnet?: boolean }): boolean =>
  (exchange === 'lighter' || exchange === 'lighter-robinhood') &&
  resolveLighterDeploymentProfile({ exchangeName: exchange, testnet: options.testnet }).usesRestProxy;

const LIGHTER_READ_AUTH_PATHS = new Set([
  '/api/v1/trades',
  '/api/v1/accountInactiveOrders',
  '/api/v1/accountActiveOrders',
  '/api/v1/positionFunding',
  '/api/v1/accountLimits',
]);
const LIGHTER_TIER_AUTH_PATH = '/api/v1/changeAccountTier';
// sign.rs AuthRecipe: deadline:account:key-index:hex(80-byte Schnorr signature).
const LIGHTER_READ_TOKEN = /^\d{1,20}:\d{1,20}:\d{1,3}:[a-f0-9]{160}$/i;

const allowsLighterToken = (
  exchange: ExchangeName,
  testnet: boolean | undefined,
  method: string,
  url: string,
  value: string,
): boolean => {
  if (!payloadSignedViaPublicProxy(exchange, { testnet }) || !LIGHTER_READ_TOKEN.test(value)) return false;
  const target = new URL(url);
  const profile = resolveLighterDeploymentProfile({ exchangeName: exchange, testnet });
  if (target.origin !== profile.apiBaseUrl || target.username || target.password) return false;
  return (
    (method.toUpperCase() === 'GET' && LIGHTER_READ_AUTH_PATHS.has(target.pathname)) ||
    (method.toUpperCase() === 'POST' && target.pathname === LIGHTER_TIER_AUTH_PATH)
  );
};

// GRVT's login answers with a `gravity=` session; its own `Cookie: rm=true` carries none.
const SESSION_COOKIE = /(?:^|;)\s*gravity=/i;

export interface PublicProxyCheck {
  exchange: ExchangeName;
  method: string;
  testnet?: boolean;
  publicProxy: string | undefined;
  /** The URL the request will actually be sent to. */
  target: string;
  /** The venue URL, whose query carries any signature. */
  url?: string;
  headers?: Record<string, string>;
  intent?: 'public' | 'primary' | 'secondary';
  credentialHeaders: ReadonlySet<string>;
  /** `payloadSignedViaPublicProxy`: its credentialed intent and signed payload are expected there. */
  payloadSigned?: boolean;
}

/** Throws before any I/O when a credentialed request would go to the public proxy. */
export const refuseSignedPublicProxy = ({
  exchange,
  method,
  testnet,
  publicProxy,
  target,
  url,
  headers,
  intent,
  credentialHeaders,
  payloadSigned = false,
}: PublicProxyCheck): void => {
  if (!publicProxy || !target.startsWith(`${publicProxy}/`)) return;
  const venueUrl = url ?? target.slice(publicProxy.length + 1);
  const credentialHeader = Object.entries(headers ?? {}).some(
    ([name, value]) =>
      Boolean(value) &&
      (credentialHeaders.has(name.toLowerCase()) || (name.toLowerCase() === 'cookie' && SESSION_COOKIE.test(value))) &&
      !(
        payloadSigned &&
        name.toLowerCase() === 'authorization' &&
        allowsLighterToken(exchange, testnet, method, venueUrl, value)
      ),
  );
  const query = [...new URL(venueUrl).searchParams.entries()];
  const signatureParam = query.some(([name]) => name.toLowerCase() === 'signature' || name.toLowerCase() === 'sign');
  const queryToken = query.some(
    ([name, value]) =>
      name.toLowerCase() === 'auth' &&
      !(payloadSigned && allowsLighterToken(exchange, testnet, method, venueUrl, value)),
  );
  const credentialed =
    credentialHeader ||
    signatureParam ||
    queryToken ||
    (!payloadSigned && (intent === 'primary' || intent === 'secondary'));
  if (credentialed) throw new SignedPublicProxyRefusal(exchange, venueUrl);
};
