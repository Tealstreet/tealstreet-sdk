import type { ExchangeName } from '../types';

import { resolveLighterDeploymentProfile } from '../exchanges/lighter/lighter.types';

/**
 * The public CORS proxy is keyless egress for public reads. A signed or credentialed
 * request must never be sent toward it: the venue key would leave the private fleet,
 * and the proxy's allowlist refuses private paths anyway.
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

const SIGNATURE_PARAM = /[?&](signature|sign)=/i;

export interface PublicProxyCheck {
  exchange: ExchangeName;
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
  publicProxy,
  target,
  url,
  headers,
  intent,
  credentialHeaders,
  payloadSigned = false,
}: PublicProxyCheck): void => {
  if (!publicProxy || !target.startsWith(`${publicProxy}/`)) return;
  // Even a payload-signed venue never sends a credential header there.
  const credentialHeader = Object.entries(headers ?? {}).some(
    ([name, value]) => Boolean(value) && credentialHeaders.has(name.toLowerCase()),
  );
  const credentialed =
    credentialHeader ||
    (!payloadSigned && (intent === 'primary' || intent === 'secondary' || SIGNATURE_PARAM.test(url ?? target)));
  if (credentialed) throw new SignedPublicProxyRefusal(exchange, url ?? target.slice(publicProxy.length + 1));
};
