export {};
// Only synthetic profiles created by selected-config-migration.test.ts are allowed.
const root = process.env.TEALSTREET_CONFIG_DIR;
if (!root?.includes('cli-owner-selected-')) throw new Error('An isolated test directory is required');
globalThis.fetch = Object.assign(
  async () => {
    throw new Error('Network forbidden');
  },
  {
    preconnect() {
      throw new Error('Network forbidden');
    },
  },
);
const { existsSync, writeFileSync } = await import('node:fs');
const config = await import('./config');
const storage = await import('./storage-context');
const hooks = await import('./listener/hooks-store');
if (process.argv[2] === 'simulate-skipped-config') {
  // Reproduce the previous version's empty config alongside a completion marker.
  writeFileSync(storage.getStoragePath('config.json'), JSON.stringify({ accounts: [] }), { mode: 0o600 });
}
const value = config.loadConfig();
console.log(
  JSON.stringify({
    accounts: config.listAccounts().map((account) => account.name),
    hasAuth: !!value.auth,
    defaultAccount: value.defaultAccount,
    requiresReentry: storage.legacyDataRequiresReentry(),
    migrationMarker: existsSync(storage.getStoragePath('.legacy-migrated')),
    inheritedHook: hooks.loadHooks().some((hook) => hook.name === 'owned-hook'),
  }),
);
process.exit(0);
