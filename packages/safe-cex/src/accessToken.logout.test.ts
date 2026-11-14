import { afterEach, expect, it, vi } from 'vitest';

vi.mock('./proxyTokenClient', () => {
  let token = '';
  return {
    setToken: (value: string) => {
      token = value;
    },
    getCurrentToken: () => token,
    getProxyToken: async () => token,
    refreshProxyToken: async () => token,
    initializeProxyTokenClient: async () => {},
  };
});
afterEach(() => vi.resetModules());
it('invalidates a refresh started before logout or user replacement', async () => {
  const tokens = await import('./accessToken');
  const client = await import('./proxyTokenClient');
  let finish: (value: string) => void = () => {};
  tokens.setTokenRefreshFunction(
    () =>
      new Promise<string>((resolve) => {
        finish = resolve;
      }),
  );
  const pending = tokens.refreshProxyToken();
  await Promise.resolve();
  tokens.setProxyToken('');
  finish('fixture-A-token');
  await expect(pending).rejects.toThrow('session changed');
  expect(tokens.currentProxyToken).toBe('');
  expect(client.getCurrentToken()).toBe('');
});
