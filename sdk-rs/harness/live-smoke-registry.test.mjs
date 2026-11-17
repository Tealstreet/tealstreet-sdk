import assert from 'node:assert/strict';
import test from 'node:test';
import { VENUES, venueOf, replayVenueOf } from './venues.mjs';
import { LIVE_SMOKE_VENUES, liveSmokeCredentialSets, liveSmokeProfile } from './live-smoke-venues.mjs';
import { betaVenueFamily } from '../hosts/wasm/js/beta-venues.mjs';
import { expectedGateCases } from './ci-gate-suites.mjs';

test('admitted Spot uses production replay while manual smoke stays separately authorized', () => {
  const fixtures = Object.keys(VENUES).filter(name => VENUES[name].rustFixture);
  assert.deepEqual(fixtures, [], 'production replay must not use qualification constructors');
  assert.equal(venueOf('bingxspot'), replayVenueOf('bingxspot'));
  assert.equal(betaVenueFamily('bingxspot'), 'bingx');
  assert.equal(LIVE_SMOKE_VENUES.includes('bingxspot'), false);
  assert.equal(liveSmokeCredentialSets().some(set => set.venues.includes('bingxspot')), false);
  for (const name of Object.keys(VENUES)) assert.equal(venueOf(name), replayVenueOf(name), name);
  assert.equal(replayVenueOf('bingxspot').rustFixture, undefined);
  assert.deepEqual(expectedGateCases('bingx').map(c => c.venue), ['bingx', 'bingx', 'bingxspot', 'bingxspot']);
});

test('neither lookup admits unknown or inherited object members', () => {
  for (const lookup of [venueOf, replayVenueOf])
    for (const name of ['not-a-venue', '__proto__', 'constructor', 'toString'])
      assert.throws(() => lookup(name), /unknown venue/);
});

test('Spot qualification never authorizes an ordinary profile or credential access', () => {
  let reads = 0;
  const env = new Proxy({}, { get() { reads++; throw new Error('unexpected credential access'); } });
  const credentials = new Proxy({}, { ownKeys() { reads++; throw new Error('unexpected stdin credential access'); } });
  for (const mode of [undefined, 'testnet', 'demo', 'mainnet']) {
    for (const options of [{}, { allowMissing: true }, { enableBingxSpot: true }, { authorized: true },
      { enableBingxSpot: 1, authorized: true }, { enableBingxSpot: true, authorized: 1 }]) {
      assert.throws(() => liveSmokeProfile('bingxspot', mode, env, { ...options, credentials }), /no reviewed.*opt-in.*authorization/);
    }
  }
  assert.equal(reads, 0);
  // The existing explicitly authorized smoke-only profile remains available,
  // with separate Spot credentials; ordinary factory admission is independent of smoke authorization.
  const profile = liveSmokeProfile('bingxspot', 'mainnet', {}, { enableBingxSpot: true, authorized: true,
    credentials: { KEY: 'fixture-spot-key', SECRET: 'fixture-spot-secret' } });
  assert.equal(profile.credentialEnv.key, 'RUST_LIVE_SMOKE_BINGX_SPOT_MAINNET_KEY');
  assert.equal(profile.options.key, 'fixture-spot-key');
  assert.throws(() => liveSmokeProfile('bingxspot', 'mainnet', env), /no reviewed/);
  assert.equal(reads, 0, 'authorization is never sticky');
});
