import type { EngineEvent } from '@tealstreet/safe-cex-wasm/host';

/** One actionable warning for this CLI session, shared across account connections. */
export function createClockSkewWarning(warn: (message: string) => void, platform: NodeJS.Platform) {
  let warned = false;
  return (event: EngineEvent): void => {
    if (warned || event.code !== 'bybit.clock.corrected' || event.level !== 'info') return;
    const offset = event.fields?.offsetMs;
    if (
      typeof offset !== 'number' ||
      !Number.isFinite(offset) ||
      Math.abs(offset) < 1000 ||
      Math.abs(offset) > Number.MAX_SAFE_INTEGER
    )
      return;
    const magnitude = Math.abs(offset);
    const duration =
      magnitude >= 60_000 ? `${(magnitude / 60_000).toFixed(1)} min` : `${(magnitude / 1000).toFixed(1)} s`;
    const action = platform === 'win32' ? ' (Windows: Settings → Time & language → Date & time → Sync now)' : '';
    warned = true;
    warn(`Your system clock is ${duration} ${offset < 0 ? 'fast' : 'slow'}; sync your system time${action}.`);
  };
}

export const warnAboutClockSkew = createClockSkewWarning((message) => console.warn(message), process.platform);
