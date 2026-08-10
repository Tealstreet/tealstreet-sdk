/**
 * What a finding does to the module carrying it.
 *
 * `block` — refuse the save outright, in any tier. Reserved for evidence of
 *   intent: constructs with no legitimate use, no observed corpus usage, and
 *   which mostly would not work anyway under an opaque origin.
 *
 * `local-only` — legitimate for someone who knows what they are doing, but the
 *   module may never travel. It runs on the device where it was written, in a
 *   frame with relaxed network rules, and is NEVER synced to the server.
 *   Everything that reaches the network or loads foreign code lands here.
 *
 * `flag` — evidence, never a gate. Recorded and handed to the model, but on its
 *   own it changes nothing. Either it works fine inside the frame, or it is
 *   already dead by virtue of the opaque origin.
 */
export type ScreenSeverity = 'block' | 'local-only' | 'flag';

/**
 * Which sandbox a module has earned.
 *
 * `strict` is the only tier that syncs — and syncing is exactly what makes a
 * module reachable by someone else's stolen session, which is the whole threat.
 * A `local-only` module cannot be delivered by an attacker because it is never
 * stored anywhere an attacker can write.
 *
 * Note that `local-only` relaxes `connect-src` and `script-src` ONLY. The
 * opaque origin is kept in both tiers, so the exchange API keys in the host's
 * localStorage stay unreachable even from code the user has explicitly
 * trusted. Advanced mode is still strictly safer than today's same-origin
 * `eval`.
 */
export type ScreenTier = 'strict' | 'local-only' | 'blocked';

export interface CircumventionFinding {
  /** Stable id, safe to grep for in logs and to alert on. */
  rule: string;
  severity: ScreenSeverity;
  /** One sentence, written for the module author, not for us. */
  detail: string;
  /** 1-based, for pointing an editor at it. */
  line: number;
  /** The offending expression, bounded. Never the whole file. */
  snippet: string;
}
