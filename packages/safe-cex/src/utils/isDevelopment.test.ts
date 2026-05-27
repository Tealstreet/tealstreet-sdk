import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

describe('isDevelopment', () => {
  const originalEnv = process.env.NODE_ENV;

  afterEach(() => {
    process.env.NODE_ENV = originalEnv;
    vi.resetModules();
  });

  it('should return true when NODE_ENV is not production', async () => {
    process.env.NODE_ENV = 'development';
    vi.resetModules();
    const { isDevelopment } = await import('./isDevelopment');
    expect(isDevelopment).toBe(true);
  });

  it('should return true when NODE_ENV is test', async () => {
    process.env.NODE_ENV = 'test';
    vi.resetModules();
    const { isDevelopment } = await import('./isDevelopment');
    expect(isDevelopment).toBe(true);
  });

  it('should return false when NODE_ENV is production', async () => {
    process.env.NODE_ENV = 'production';
    vi.resetModules();
    const { isDevelopment } = await import('./isDevelopment');
    expect(isDevelopment).toBe(false);
  });

  it('should return true when NODE_ENV is undefined', async () => {
    delete process.env.NODE_ENV;
    vi.resetModules();
    const { isDevelopment } = await import('./isDevelopment');
    expect(isDevelopment).toBe(true);
  });
});
