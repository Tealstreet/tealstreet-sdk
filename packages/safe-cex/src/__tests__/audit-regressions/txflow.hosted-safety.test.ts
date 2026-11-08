import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions, RemoteSigner } from '../../types';
import { expect, it, vi } from 'vitest';
import { DelegatedSignerSetup } from '../../exchanges/rust/delegatedSignerSetup';
import { deriveBridgeConstruction, deriveRemoteSigningPolicy } from '../../exchanges/rust/construction';

const unusedEngineMethod = (): never => { throw new Error('Unexpected engine call'); };

it('TxFlow hosted construction passes references and public addresses, never the agent scalar', () => {
  const remoteSigner: RemoteSigner = {
    sign: vi.fn(), isAvailable: () => true, onAvailabilityChange: () => () => {},
  };
  const input: ExchangeOptions = { testnet: true, key: '0x0202020202020202020202020202020202020202',
    applicationId: '0x1111111111111111111111111111111111111111',
    secret: 'audit-secret-sentinel', remoteSigner };
  const setup = new DelegatedSignerSetup('txflow', input);
  let captured = '';
  class Host implements WasmEngineLike {
    capabilities = unusedEngineMethod;
    start = unusedEngineMethod;
    command = unusedEngineMethod;
    takeFrame = unusedEngineMethod;
    snapshotFrame = unusedEngineMethod;
    wsOpen = unusedEngineMethod;
    wsMessage = unusedEngineMethod;
    wsClose = unusedEngineMethod;
    wsRouteExhausted = unusedEngineMethod;
    httpStart = unusedEngineMethod;
    httpChunk = unusedEngineMethod;
    httpEnd = unusedEngineMethod;
    httpFailed = unusedEngineMethod;
    timer = unusedEngineMethod;
    constructor(_venue: string, json: string) { captured = json; }
  }
  setup.construct(Host, 'txflow', JSON.stringify(input));
  expect(captured).not.toContain('audit-secret-sentinel');
  expect(JSON.parse(captured)).not.toHaveProperty('secret');
  expect(JSON.parse(captured)).toMatchObject({ testnet: true, applicationId: input.applicationId,
    remoteSigning: true, signing: { primary: { signing: expect.any(String) } } });
  expect(setup.signerAlgorithms).toEqual(['secp256k1Digest']);
  expect(deriveRemoteSigningPolicy('txflow')).toMatchObject({ enabled: true, payloadSupported: false });
  expect(deriveBridgeConstruction('txflow', input).dispatchSigning).toBe(true);
  setup.dispose();
});
