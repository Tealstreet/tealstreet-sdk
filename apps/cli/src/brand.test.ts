import { describe, expect, it } from 'vitest';
import { BRAND, releaseAssetName } from './brand';

// Dev runs and tests carry no TEALSTREET_BRAND define, so the default must be
// tealstreet or every existing test that pins `~/.tealstreet` and the
// `tealstreet-*` asset names would start failing for the wrong reason.
describe('brand', () => {
  it('defaults to tealstreet without a build-time define', () => {
    expect(BRAND.id).toBe('tealstreet');
    expect(BRAND.configDirName).toBe('.tealstreet');
    expect(BRAND.releaseRepo).toBe('Tealstreet/cli');
  });

  it('derives release asset names the way build.sh emits them', () => {
    expect(releaseAssetName(BRAND, 'darwin-arm64')).toBe('tealstreet-darwin-arm64');
    expect(releaseAssetName(BRAND, 'windows-x64')).toBe('tealstreet-windows-x64.exe');
    expect(releaseAssetName({ ...BRAND, binName: 'disrupt' }, 'linux-x64')).toBe('disrupt-linux-x64');
  });
});
