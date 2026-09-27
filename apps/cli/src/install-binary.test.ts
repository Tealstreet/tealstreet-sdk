import { createRequire } from 'node:module';

import { afterEach, describe, expect, it } from 'vitest';

const require = createRequire(import.meta.url);
const installer = require('../scripts/install-binary.cjs');

/**
 * The npm launcher `spawnSync`s whatever `getBinaryPath()` returns. On Windows
 * that path MUST end in `.exe` or the process never starts — and the release
 * asset carries the suffix because `bun build --compile` appends it to a
 * Windows target. Both halves are derived from one `assetName()`, so the
 * download URL, the SHA256SUMS lookup and the cached filename cannot drift
 * apart. That drift is silent: it presents as a 404 or an unrunnable file,
 * not as a build failure.
 */
describe('assetName', () => {
  it('appends .exe for a Windows target', () => {
    expect(installer.assetName('windows-x64')).toBe('tealstreet-windows-x64.exe');
  });

  it('leaves POSIX targets bare', () => {
    expect(installer.assetName('darwin-arm64')).toBe('tealstreet-darwin-arm64');
    expect(installer.assetName('darwin-x64')).toBe('tealstreet-darwin-x64');
    expect(installer.assetName('linux-x64')).toBe('tealstreet-linux-x64');
    expect(installer.assetName('linux-arm64')).toBe('tealstreet-linux-arm64');
  });
});

describe('resolvePlatform', () => {
  const realPlatform = process.platform;
  const realArch = process.arch;

  const setPlatform = (platform: string, arch: string) => {
    Object.defineProperty(process, 'platform', { value: platform, configurable: true });
    Object.defineProperty(process, 'arch', { value: arch, configurable: true });
  };

  afterEach(() => setPlatform(realPlatform, realArch));

  it('maps win32 rather than rejecting it as unsupported', () => {
    setPlatform('win32', 'x64');
    expect(installer.resolvePlatform()).toBe('windows-x64');
    expect(installer.getBinaryPath().endsWith('tealstreet-windows-x64.exe')).toBe(true);
  });

  it('still maps the POSIX targets', () => {
    setPlatform('darwin', 'arm64');
    expect(installer.resolvePlatform()).toBe('darwin-arm64');
    setPlatform('linux', 'x64');
    expect(installer.resolvePlatform()).toBe('linux-x64');
  });

  it('rejects a platform with no published binary', () => {
    setPlatform('aix', 'x64');
    expect(() => installer.resolvePlatform()).toThrow(/Unsupported platform/);
  });
});
