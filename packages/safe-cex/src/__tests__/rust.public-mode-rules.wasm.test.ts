import type { WasmEngineModule } from '../exchanges/rust/rust.exchange';
import type { ExchangeName, ExchangeOptions } from '../types';

import { beforeAll, expect, it } from 'vitest';

import { derivePublicMode, deriveSignerSetupPolicy } from '../exchanges/rust/construction';
import { loadNodeEngine } from '../exchanges/rust/loadNodeEngine';
import { LocalSignerSetup } from '../exchanges/rust/localSignerSetup';
import { RUST_VENUES } from '../exchanges/rust/rust.exchange';

const wallet = '0x1111111111111111111111111111111111111111';
const privateKey = `0x${'12'.repeat(32)}`;
let wasm: WasmEngineModule;
beforeAll(async () => {
  wasm = await loadNodeEngine();
});

const cases: ExchangeOptions[] = [
  { key: '', secret: '' },
  { key: 'key', secret: '' },
  { key: '', secret: 'local' },
  { key: 'key', secret: 'local' },
  { key: 'key', secret: '', payloadSigner: (() => {}) as unknown as ExchangeOptions['payloadSigner'] },
  { key: 'key', secret: '', remoteSigner: (() => {}) as unknown as ExchangeOptions['remoteSigner'] },
  { key: '', secret: 'local', applicationId: 'app' },
  { key: '', secret: '', vault: 'vault' },
  { key: '', secret: '', applicationId: 'app' },
  { key: 'key', secret: '', paradexAddress: 'address', subkeyAddress: 'subkey' },
  { key: '', secret: '', paradexAddress: 'address', subkeyAddress: 'subkey' },
  { key: '', secret: 'local', apiKeyIndex: '0' },
  { key: '', secret: '', apiKeyIndex: '0', remoteSigner: (() => {}) as unknown as ExchangeOptions['remoteSigner'] },
  { key: '', secret: '', apiKeyIndex: '0', payloadSigner: (() => {}) as unknown as ExchangeOptions['payloadSigner'] },
  { key: '', secret: 'local', apiKeyIndex: '' },
  { key: ' ', secret: ' ', applicationId: ' ', paradexAddress: ' ', subkeyAddress: ' ', apiKeyIndex: ' ' },
  { key: wallet, secret: privateKey, applicationId: wallet },
  { key: wallet, secret: privateKey, applicationId: wallet, extra: { asterdexApiVersion: 'v1' } },
  { key: 'key', secret: 'local', extra: { asterdexApiVersion: 'v3' } },
  { key: 'key', secret: '', auth: { userAddress: wallet, signerAddress: wallet, privateKey } },
  { key: '', secret: '', auth: { userAddress: wallet, signerAddress: wallet, privateKey } },
  { key: wallet, secret: privateKey, applicationId: wallet, auth: { privateKey: 'invalid' } },
  { key: wallet, secret: privateKey, applicationId: wallet, auth: { privateKey: ' ' } },
  { key: '\u0085', secret: '\u0085' },
  { key: '\ufeff', secret: '\ufeff' },
  { key: ' \t\n', secret: ' \t\n' },
  { key: 'key', secret: ' ' },
];

const credentialPresence = (venue: ExchangeName, options: ExchangeOptions) => {
  const { trimCredential, trimSigning } = deriveSignerSetupPolicy(venue).primary ?? {};
  // This resolver probes presence only; passphrase/provider validation occurs during construction.
  const setup = new LocalSignerSetup(options, { primary: { trimCredential, trimSigning } });
  try {
    return setup.credentialPresence;
  } finally {
    setup.dispose();
  }
};

it.each([...RUST_VENUES])('compiled public-mode rules match trusted signer-setup presence for %s', (venue) => {
  for (const options of cases) {
    for (const requested of [undefined, false, true]) {
      const input = { ...options, publicDataOnly: requested };
      const expected = derivePublicMode(venue, input, undefined, credentialPresence(venue, input));
      const native = JSON.parse(
        wasm.resolveConstruction!(
          venue,
          JSON.stringify({
            key: options.key,
            secret: options.secret,
            applicationId: options.applicationId,
            vault: options.vault,
            paradexAddress: options.paradexAddress,
            subkeyAddress: options.subkeyAddress,
            // Deliberately disagree: raw intent must override the old derived hint.
            publicDataOnly: !expected.publicDataOnly,
            publicMode: {
              requested: requested ?? null,
              remoteSigner: Boolean(options.remoteSigner),
              payloadSigner: Boolean(options.payloadSigner),
              apiKeyIndexPresent: Boolean(options.apiKeyIndex),
            },
            asterdex: { ...options.auth, apiVersion: options.extra?.asterdexApiVersion },
          }),
        ),
      );
      expect(native, `${venue} case ${cases.indexOf(options)} requested ${requested}`).toMatchObject(expected);
    }
  }
});
