export {};
// Synthetic subprocess fixture. Never read the operator's profile or use the network.
const root = process.env.TEALSTREET_CONFIG_DIR;
if (!root?.includes('cli-owner-')) throw new Error('An isolated test directory is required');
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
const config = await import('./config');
const storage = await import('./storage-context');
const { FilePreprocessStore } = await import('./preprocess-store');
const hooks = await import('./listener/hooks-store');
const state = await import('./listener/state');
const store = new FilePreprocessStore();
if (process.argv[2] === 'local-seed') {
  config.addAccount({ name: 'Local', exchange: 'bybit', apiKey: 'synthetic-local', apiSecret: 'synthetic-local' });
  store.setVar('local', 'synthetic-local');
  store.setAlias('local', 'synthetic-local');
}
if (process.argv[2] === 'login') config.saveConfig({ accounts: [], auth: { userId: 'B', accessToken: 'synthetic-B' } });
console.log(
  JSON.stringify({
    owner: storage.getStorageOwner(),
    defaultAccount: config.loadConfig().defaultAccount,
    accounts: config.listAccounts().map((a) => a.name),
    localVar: store.getVar('local'),
    localAlias: store.getAlias('local'),
    inheritedAHook: hooks.loadHooks().some((h) => h.name === 'A-private'),
    inheritedAApproval: !!state.isTrusted('A-browser'),
    inheritedAVar: store.getVar('A-private') !== undefined,
  }),
);
process.exit(0);
