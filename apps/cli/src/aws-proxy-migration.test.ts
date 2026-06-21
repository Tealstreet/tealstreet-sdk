import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('./auth', () => ({
  isLoggedIn: vi.fn(),
  getProxyTokenFromConfig: vi.fn(),
  getTealstreetApiBaseUrl: vi.fn(() => 'http://localhost:3000'),
}));

vi.mock('../../../packages/safe-cex/src/utils/transportResolution', () => ({
  setAwsProxyMigrationEnabled: vi.fn(),
}));

import { setAwsProxyMigrationEnabled } from '../../../packages/safe-cex/src/utils/transportResolution';
import { getProxyTokenFromConfig, isLoggedIn } from './auth';
import { __resetAwsProxyMigrationSyncForTest, ensureAwsProxyMigrationFlag } from './aws-proxy-migration';

const fetchMock = vi.fn();
const flagsResponse = (flags: Record<string, unknown>) =>
  Promise.resolve(new Response(JSON.stringify(flags), { status: 200 }));

describe('ensureAwsProxyMigrationFlag', () => {
  const originalFetch = global.fetch;

  beforeEach(() => {
    __resetAwsProxyMigrationSyncForTest();
    fetchMock.mockReset();
    global.fetch = fetchMock as unknown as typeof fetch;
    vi.mocked(isLoggedIn).mockReset();
    vi.mocked(getProxyTokenFromConfig).mockReset();
    vi.mocked(setAwsProxyMigrationEnabled).mockReset();
  });

  afterEach(() => {
    global.fetch = originalFetch;
  });

  it('does nothing when not logged in (leaves the gate OFF = legacy AWS)', async () => {
    vi.mocked(isLoggedIn).mockReturnValue(false);
    vi.mocked(getProxyTokenFromConfig).mockReturnValue('tok');
    await ensureAwsProxyMigrationFlag();
    expect(fetchMock).not.toHaveBeenCalled();
    expect(setAwsProxyMigrationEnabled).not.toHaveBeenCalled();
  });

  it('does nothing when there is no stored token', async () => {
    vi.mocked(isLoggedIn).mockReturnValue(true);
    vi.mocked(getProxyTokenFromConfig).mockReturnValue(null);
    await ensureAwsProxyMigrationFlag();
    expect(fetchMock).not.toHaveBeenCalled();
    expect(setAwsProxyMigrationEnabled).not.toHaveBeenCalled();
  });

  it('fetches /api/feature-flags with the proxy token and enables the gate when the flag is true', async () => {
    vi.mocked(isLoggedIn).mockReturnValue(true);
    vi.mocked(getProxyTokenFromConfig).mockReturnValue('tok-abc');
    fetchMock.mockReturnValue(flagsResponse({ ENABLE_AWS_PROXY_MIGRATION: true }));

    await ensureAwsProxyMigrationFlag();

    const [url, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe('http://localhost:3000/api/feature-flags');
    expect((init.headers as Record<string, string>)['authorization']).toBe('Bearer tok-abc');
    expect(setAwsProxyMigrationEnabled).toHaveBeenCalledWith(true);
  });

  it('leaves the gate OFF when the flag is absent or false', async () => {
    vi.mocked(isLoggedIn).mockReturnValue(true);
    vi.mocked(getProxyTokenFromConfig).mockReturnValue('tok');
    fetchMock.mockReturnValue(flagsResponse({ SOME_OTHER_FLAG: true }));
    await ensureAwsProxyMigrationFlag();
    expect(setAwsProxyMigrationEnabled).toHaveBeenCalledWith(false);
  });

  it('is memoized — only fetches once per process', async () => {
    vi.mocked(isLoggedIn).mockReturnValue(true);
    vi.mocked(getProxyTokenFromConfig).mockReturnValue('tok');
    fetchMock.mockReturnValue(flagsResponse({ ENABLE_AWS_PROXY_MIGRATION: true }));
    await ensureAwsProxyMigrationFlag();
    await ensureAwsProxyMigrationFlag();
    await ensureAwsProxyMigrationFlag();
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it('swallows fetch rejections (leaves the gate OFF)', async () => {
    vi.mocked(isLoggedIn).mockReturnValue(true);
    vi.mocked(getProxyTokenFromConfig).mockReturnValue('tok');
    fetchMock.mockRejectedValue(new Error('network down'));
    await expect(ensureAwsProxyMigrationFlag()).resolves.toBeUndefined();
    expect(setAwsProxyMigrationEnabled).not.toHaveBeenCalled();
  });

  it('ignores a non-OK response', async () => {
    vi.mocked(isLoggedIn).mockReturnValue(true);
    vi.mocked(getProxyTokenFromConfig).mockReturnValue('tok');
    fetchMock.mockReturnValue(Promise.resolve(new Response('nope', { status: 401 })));
    await ensureAwsProxyMigrationFlag();
    expect(setAwsProxyMigrationEnabled).not.toHaveBeenCalled();
  });
});
