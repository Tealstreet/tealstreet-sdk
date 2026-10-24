/**
 * What a finding does to the module carrying it.
 *
 * `block` — refuse the save outright, in any tier. Reserved for evidence of
 *   intent: constructs with no legitimate use, no observed corpus usage, and
 *   which mostly would not work anyway under an opaque origin.
 *
 * `standard` — legitimate, but needs the network. The module runs in the
 *   `standard` frame (`connect-src https:`) and syncs, commits and publishes
 *   like any other. Everything that reaches the network or loads foreign code
 *   lands here.
 *
 * `flag` — evidence, never a gate. Recorded and handed to the model, but on its
 *   own it changes nothing. Either it works fine inside the frame, or it is
 *   already dead by virtue of the opaque origin.
 */
export type ScreenSeverity = 'block' | 'standard' | 'flag';

/**
 * Which sandbox a module has earned. Both `strict` and `standard` sync.
 *
 * `standard` relaxes `connect-src` and `script-src` ONLY. The opaque origin is
 * kept in both tiers, so the exchange API keys never enter the frame — the one
 * asset the threat model protects. A standard module sending position or order
 * data over the network, or loading a third-party script that changes after it
 * was screened, is an ACCEPTED risk (owner decision, 2026-10-22).
 */
export type ScreenTier = 'strict' | 'standard' | 'blocked';

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
