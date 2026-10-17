/**
 * Build-time branding. One binary source, two brands.
 *
 * `scripts/build.sh <brand>` inlines `process.env.TEALSTREET_BRAND` via
 * `--define`; dev runs and tests have no define and resolve to tealstreet, so
 * nothing changes for them. The web app routes Auth0 tenants by request HOST
 * (`apps/web/src/utils/auth/auth0.ts` → `tenantFromHost`), so pointing
 * `webUrl` at the whitelabel domain is the entire auth change: `/api/cli/login`
 * there issues a proxy token whose userId already carries the tenant prefix.
 *
 * Infrastructure names stay as they are on every brand — `TEALSTREET_*` env
 * vars, `--tealstreet-proxy` flags, the relay URLs and the HoW `Origin` header
 * — those name the shared tunnel, not the product.
 */
export type BrandId = 'tealstreet' | 'disrupt';

export interface Brand {
  id: BrandId;
  /** Product name in user-facing copy. */
  displayName: string;
  /** Executable / Commander name, release asset prefix. */
  binName: string;
  /** Directory under $HOME holding config, logs, scripts, recording.db. */
  configDirName: string;
  /** Web app the CLI logs in through; release builds also inline TEALSTREET_API. */
  webUrl: string;
  docsUrl: string;
  /** Public GitHub repo holding binary releases (owner/name). */
  releaseRepo: string;
  /** npm package for self-update, or null when the brand is not on npm. */
  npmPackage: string | null;
  /** Homebrew formula for self-update, or null when the brand has no tap. */
  brewFormula: string | null;
}

export const BRANDS: Record<BrandId, Brand> = {
  tealstreet: {
    id: 'tealstreet',
    displayName: 'Tealstreet',
    binName: 'tealstreet',
    configDirName: '.tealstreet',
    webUrl: 'https://preview.tealstreet.io',
    docsUrl: 'https://docs.tealstreet.io/cli',
    releaseRepo: 'Tealstreet/cli',
    npmPackage: '@tealstreet/cli',
    brewFormula: 'tealstreet',
  },
  disrupt: {
    id: 'disrupt',
    displayName: 'Disrupt',
    binName: 'disrupt',
    configDirName: '.disrupt',
    webUrl: 'https://disrupt.trade',
    docsUrl: 'https://github.com/DisruptTrade/cli#readme',
    releaseRepo: 'DisruptTrade/cli',
    npmPackage: null,
    brewFormula: null,
  },
};

function resolveBrandId(): BrandId {
  const raw = process.env.TEALSTREET_BRAND;
  if (raw === 'disrupt') return 'disrupt';
  return 'tealstreet';
}

export const BRAND: Brand = BRANDS[resolveBrandId()];

/** Release-asset name for a platform target, exactly as build.sh emits it. */
export function releaseAssetName(brand: Brand, target: string): string {
  return target.startsWith('windows') ? `${brand.binName}-${target}.exe` : `${brand.binName}-${target}`;
}
