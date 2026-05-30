import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('./auth', () => ({
  isLoggedIn: vi.fn(),
  getProxyTokenFromConfig: vi.fn(),
  getTealstreetApiBaseUrl: vi.fn(() => 'http://localhost:3000'),
}));

import { getProxyTokenFromConfig, isLoggedIn } from './auth';
import { __resetCheckinFiredForTest, fireCheckin } from './checkin';

const fetchMock = vi.fn(() => Promise.resolve(new Response(null, { status: 200 })));

describe('fireCheckin', () => {
  const originalFetch = global.fetch;
  const originalPlatform = process.platform;

  const setPlatform = (platform: NodeJS.Platform) => {
    Object.defineProperty(process, 'platform', { value: platform, configurable: true });
  };

  beforeEach(() => {
    __resetCheckinFiredForTest();
    fetchMock.mockClear();
    global.fetch = fetchMock as unknown as typeof fetch;
    vi.mocked(isLoggedIn).mockReset();
    vi.mocked(getProxyTokenFromConfig).mockReset();
    setPlatform('darwin');
  });

  afterEach(() => {
    global.fetch = originalFetch;
    setPlatform(originalPlatform);
  });

  it('skips when not logged in', () => {
    vi.mocked(isLoggedIn).mockReturnValue(false);
    vi.mocked(getProxyTokenFromConfig).mockReturnValue('tok-should-not-be-read');
    fireCheckin();
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('skips when there is no stored token', () => {
    vi.mocked(isLoggedIn).mockReturnValue(true);
    vi.mocked(getProxyTokenFromConfig).mockReturnValue(null);
    fireCheckin();
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('skips on platforms the web gate does not know about', () => {
    setPlatform('freebsd');
    vi.mocked(isLoggedIn).mockReturnValue(true);
    vi.mocked(getProxyTokenFromConfig).mockReturnValue('tok');
    fireCheckin();
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('POSTs to /api/cli/checkin with Bearer + platform body when ready', () => {
    vi.mocked(isLoggedIn).mockReturnValue(true);
    vi.mocked(getProxyTokenFromConfig).mockReturnValue('tok-abc');
    fireCheckin();
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe('http://localhost:3000/api/cli/checkin');
    expect(init.method).toBe('POST');
    expect((init.headers as Record<string, string>)['authorization']).toBe('Bearer tok-abc');
    expect((init.headers as Record<string, string>)['content-type']).toBe('application/json');
    expect(JSON.parse(init.body as string)).toEqual({ platform: 'darwin' });
  });

  it('only fires once per process — second call is a no-op', () => {
    vi.mocked(isLoggedIn).mockReturnValue(true);
    vi.mocked(getProxyTokenFromConfig).mockReturnValue('tok');
    fireCheckin();
    fireCheckin();
    fireCheckin();
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it('swallows fetch rejections', async () => {
    fetchMock.mockImplementationOnce(() => Promise.reject(new Error('network down')));
    vi.mocked(isLoggedIn).mockReturnValue(true);
    vi.mocked(getProxyTokenFromConfig).mockReturnValue('tok');
    expect(() => fireCheckin()).not.toThrow();
    // Give the microtask queue a tick to surface any unhandled rejection.
    await Promise.resolve();
  });
});
