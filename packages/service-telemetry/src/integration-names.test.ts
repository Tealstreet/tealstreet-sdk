import { getDefaultIntegrations } from '@sentry/bun';
import { describe, expect, it } from 'vitest';

import { DROPPED_INTEGRATIONS } from './init';
import { OUTGOING_REQUEST_BREADCRUMB_INTEGRATIONS } from './integrations';

/**
 * Deliberately does NOT mock `@sentry/bun`.
 *
 * The filter in `init.ts` matches integrations by NAME, so every other test in
 * this package proves nothing about it: they fabricate the names they pass in.
 * A typo, or an SDK upgrade that renames one, would leave the integration
 * enabled — and the ones being dropped are a credential-breadcrumb path and an
 * unthrottled event source — while the suite stayed green.
 *
 * This reads the names the installed SDK really registers.
 */
describe('the drop list matches the installed SDK', () => {
  const defaults = getDefaultIntegrations({}).map((integration) => integration.name);
  const defaultSet = new Set(defaults);

  // v10 -> v11 renamed/replaced these defaults. Keep both names in the runtime
  // filters so either SDK generation remains hardened while Dependabot rolls.
  const versionedDropNames = new Set(['NodeFetch', 'Fetch', 'BunHttpServer']);

  it('every dropped name is a current default or an SDK-rename compatibility name', () => {
    for (const dropped of DROPPED_INTEGRATIONS) {
      expect(
        defaultSet.has(dropped) || versionedDropNames.has(dropped),
        `${dropped} is not a default and is not documented as an SDK rename compatibility name`,
      ).toBe(true);
    }
  });

  it('drops the credential-breadcrumb integrations', () => {
    // Console carries `alert()`'s own unscrubbed log line; Http/NodeFetch/Fetch
    // carry signed venue REST URLs; RequestData lifts a query string onto an
    // event; BunServer/BunHttpServer capture the incoming request carrying the
    // proxy token.
    for (const name of ['Console', 'Http', 'RequestData', 'BunServer', 'BunHttpServer']) {
      if (defaultSet.has(name)) expect(DROPPED_INTEGRATIONS.has(name)).toBe(true);
    }

    const outgoingDefaults = defaults.filter((name) => OUTGOING_REQUEST_BREADCRUMB_INTEGRATIONS.has(name));
    expect(outgoingDefaults.length, 'expected an outgoing request integration default').toBeGreaterThan(0);
    for (const name of outgoingDefaults) {
      expect(DROPPED_INTEGRATIONS.has(name), `${name} should be dropped`).toBe(true);
    }
  });

  it('drops the global handlers that would bypass captureAlert', () => {
    for (const name of ['OnUncaughtException', 'OnUnhandledRejection']) {
      expect(DROPPED_INTEGRATIONS.has(name)).toBe(true);
    }
  });

  it('leaves the harmless defaults alone', () => {
    for (const names of [['InboundFilters', 'EventFilters'], ['FunctionToString'], ['LinkedErrors']]) {
      expect(
        names.some((name) => defaultSet.has(name)),
        `expected one of ${names.join(' / ')} to be a default`,
      ).toBe(true);
      for (const name of names) {
        expect(DROPPED_INTEGRATIONS.has(name)).toBe(false);
      }
    }
  });
});
