/**
 * Product branding for text that cli-core prints at runtime (REPL `help`,
 * `accounts` guidance). Defaults to Tealstreet; the standalone CLI calls
 * `setCliBrand` from its own build-time brand (`apps/cli/src/brand.ts`) at
 * startup, so a whitelabel binary never prints the Tealstreet name or a
 * Tealstreet URL. The web CLI leaves the default.
 *
 * The routing flags are accepted under BOTH spellings on every brand
 * (`--tealstreet-proxy` and `--proxy`, …); `flags` only decides which
 * spelling the help text shows.
 */
export interface CliBrand {
  /** Product name in prose: "Authenticate with <displayName>". */
  displayName: string;
  /** Where "Full reference + recipes" points. */
  docsUrl: string;
  /** Flag spelling shown in help for the three routing options. */
  flags: { proxy: string; tunnel: string; wsRoute: string };
}

export const TEALSTREET_CLI_BRAND: CliBrand = {
  displayName: 'Tealstreet',
  docsUrl: 'https://docs.tealstreet.io/cli',
  flags: { proxy: '--tealstreet-proxy', tunnel: '--tealstreet-tunnel', wsRoute: '--tealstreet-ws-route' },
};

let current: CliBrand = TEALSTREET_CLI_BRAND;

export function setCliBrand(brand: CliBrand): void {
  current = brand;
}

export function getCliBrand(): CliBrand {
  return current;
}
