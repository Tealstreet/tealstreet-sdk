import { ExchangeError } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions } from '../../types';
import {
  ASTERDEX_MAIN_WALLET_REQUIRED_MESSAGE,
  isAsterDEXAddress,
  isAsterDEXV3PrivateKey,
  resolveAsterDEXApiVersion,
  resolveAsterDEXV3AuthContext,
} from '../asterdex/asterdex.types';

/** Public config trusted setup resolves; the venue never sees key material. */
export type AsterDEXPublicSetup = Readonly<{
  apiVersion: 'v1' | 'v3';
  signerAddress?: string;
  readSignerAddress?: string;
}>;

const trimmed = (value?: string): string | undefined => value?.trim() || undefined;

/** TS half of AsterDEX trusted setup (RUST_ASTER_SIGNER_ADMISSION.md decisions
 * 4-5). Native setup maps identically; asterdex-signer-setup.golden.json pins both. */
export const mapAsterDEXSignerSetup = (options: ExchangeOptions): Readonly<{
  setupOptions: ExchangeOptions;
  secondarySigningFromPrimary: boolean;
  publicSetup: AsterDEXPublicSetup;
}> => {
  const apiVersion = resolveAsterDEXApiVersion(options);
  const { userAddress } = resolveAsterDEXV3AuthContext(options);
  if (!options.publicDataOnly && apiVersion === 'v1' && isAsterDEXAddress(trimmed(options.key))
    && isAsterDEXV3PrivateKey(options.secret) && !userAddress) {
    const message = ASTERDEX_MAIN_WALLET_REQUIRED_MESSAGE;
    throw new ExchangeError(message, { message, kind: 'invalidInput', outcome: 'notSent' });
  }
  const v3 = apiVersion === 'v3';
  const authSigner = trimmed(options.auth?.signerAddress);
  // The read agent prefers the primary wallet from auth even when a read secret is set.
  const aliased = v3 && trimmed(options.auth?.privateKey) !== undefined;
  const wallet = aliased ? options.auth?.privateKey : options.secret;
  const secretEncoding = v3 ? 'hex' : 'utf8';
  const metadata = options.localSigner ?? {};
  const secondary = options.secondary_key !== undefined && options.secondary_secret !== undefined;
  return {
    setupOptions: {
      ...options,
      // Blank is absent. V3 wallet keys are trimmed hex, as the legacy venue
      // read them; V1 HMAC secrets stay byte-exact.
      secret: trimmed(wallet) === undefined ? undefined : v3 ? trimmed(wallet) : wallet,
      secondary_key: secondary ? options.secondary_key : undefined,
      secondary_secret: secondary && !aliased ? options.secondary_secret : undefined,
      localSigner: {
        primary: { ...metadata.primary, secretEncoding },
        ...(secondary ? { secondary: { ...metadata.secondary, secretEncoding } } : {}),
      },
    } as ExchangeOptions,
    secondarySigningFromPrimary: secondary && aliased,
    publicSetup: Object.freeze({
      apiVersion,
      ...(v3 ? { signerAddress: authSigner ?? trimmed(options.key) } : {}),
      ...(v3 ? { readSignerAddress: authSigner ?? trimmed(options.secondary_key) } : {}),
    }),
  };
};
