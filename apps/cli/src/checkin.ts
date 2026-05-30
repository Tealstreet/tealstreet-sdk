import { getProxyTokenFromConfig, getTealstreetApiBaseUrl, isLoggedIn } from './auth';

const ALLOWED_PLATFORMS = new Set(['darwin', 'win32', 'linux']);

let fired = false;

/**
 * Best-effort POST /api/cli/checkin so the web app knows this user
 * has the CLI running on this platform. Fire-and-forget — never
 * throws, never blocks startup, never logs (the user doesn't care).
 *
 * Why: every web page load mounted CliListenerWorker unconditionally
 * and probed `ws://127.0.0.1:53219/ws`, producing ~3 noisy
 * connection-failed console errors for users who don't run the CLI.
 * The web side now gates that mount on a fresh
 * `settings.cli.checkins[<webPlatform>]` timestamp; this function is
 * how the timestamp gets written.
 *
 * Skips silently when:
 *   - this process has already fired (module-level latch — at most one
 *     checkin per CLI invocation, regardless of how many code paths
 *     call this)
 *   - not logged in (no token to send)
 *   - the platform isn't one the web gate knows about (Bun on a BSD
 *     etc.)
 *   - the network is down (fetch rejects; caller doesn't care)
 */
export function fireCheckin(): void {
  if (fired) return;
  fired = true;

  if (!isLoggedIn()) return;
  const token = getProxyTokenFromConfig();
  if (!token) return;

  const platform = process.platform;
  if (!ALLOWED_PLATFORMS.has(platform)) return;

  const url = `${getTealstreetApiBaseUrl()}/api/cli/checkin`;
  void fetch(url, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      authorization: `Bearer ${token}`,
    },
    body: JSON.stringify({ platform }),
  }).catch(() => {
    // best-effort; ignore network failures
  });
}

/** Test hook. Do not call from product code. */
export function __resetCheckinFiredForTest(): void {
  fired = false;
}
