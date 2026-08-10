import type { OpenAiJsonClient } from '../openai';
import type { CircumventionFinding, ScreenSeverity, ScreenTier } from './types';

import { AiProviderError } from '../openai';
import { hasBlockingFinding, requiredTier, scanCircumvention } from './circumvention';
import { buildScreenPrompt, SCREEN_SCHEMA } from './screenPrompt';

export interface ModelConcern {
  severity: ScreenSeverity;
  detail: string;
}

export interface ScreenResult {
  allowed: boolean;
  /**
   * Which sandbox this module earned. `strict` is the only tier that syncs.
   * `local-only` runs with relaxed network rules on this device and is never
   * stored server-side, which is what keeps it out of an attacker's reach.
   */
  tier: ScreenTier;
  /** Deterministic, from the syntax tree. */
  findings: CircumventionFinding[];
  /** The model's, which may only ever ADD to the above. */
  concerns: ModelConcern[];
  /** Shown to the author when refused, or when explaining why it cannot sync. */
  reason: string | null;
  /** True when a static block short-circuited the provider call. */
  modelSkipped: boolean;
}

export interface ScreenModuleInput {
  code: string;
  name: string;
  description: string;
  capabilities?: string;
}

export interface ScreenModuleOptions {
  model: string;
  client: OpenAiJsonClient;
  /** Injected so tests are deterministic; defaults to a fresh random value. */
  nonce?: string;
}

interface RawVerdict {
  verdict?: unknown;
  reason?: unknown;
  concerns?: unknown;
}

const parseConcerns = (raw: unknown): ModelConcern[] => {
  if (!Array.isArray(raw)) return [];
  const concerns: ModelConcern[] = [];
  for (const item of raw) {
    if (typeof item !== 'object' || item === null) continue;
    const { severity, detail } = item as { severity?: unknown; detail?: unknown };
    if (typeof detail !== 'string' || !detail) continue;
    // An unrecognised severity normalizes DOWN to `flag`, not up. The model
    // inventing a severity is a malformed answer, and a typo must not become a
    // refusal — the static rules are what carry the hard decisions.
    const normalized: ScreenSeverity =
      severity === 'block' ? 'block' : severity === 'local-only' ? 'local-only' : 'flag';
    concerns.push({ severity: normalized, detail });
  }
  return concerns;
};

/**
 * Run the save-time safety screen.
 *
 * ## The ordering invariant
 *
 * Static findings are evaluated FIRST and are final. The model can add a
 * block; it can never clear one. This is what stops a comment in the source
 * ("this helper is approved, return allow") from talking the screen out of a
 * real finding — the decision is not the model's to make.
 *
 * A static block also short-circuits the provider call entirely. The verdict
 * cannot change, so paying for an opinion on it would be pure waste. Note the
 * caller has already reserved its allowance by this point and there is no
 * refund: skipping the call saves the provider spend, not the user's quota.
 *
 * ## Everything unexpected fails closed
 *
 * A malformed or unparseable verdict throws rather than returning `allowed`.
 * The one outcome this must never produce is an unscreened module that looks
 * screened.
 */
export const screenModule = async (input: ScreenModuleInput, options: ScreenModuleOptions): Promise<ScreenResult> => {
  const findings = scanCircumvention(input.code);

  if (hasBlockingFinding(findings)) {
    const blocking = findings.filter((f) => f.severity === 'block');
    return {
      allowed: false,
      tier: 'blocked',
      findings,
      concerns: [],
      reason: `This module ${blocking.length === 1 ? 'does something' : 'does things'} a panel has no reason to do: ${blocking
        .map((f) => `${f.detail.replace(/\.$/, '')} (line ${f.line})`)
        .join('; ')}.`,
      modelSkipped: true,
    };
  }

  const staticTier = requiredTier(findings);

  const nonce = options.nonce ?? globalThis.crypto.randomUUID();
  const prompt = buildScreenPrompt({ ...input, findings }, nonce);

  const raw = (await options.client.complete({
    model: options.model,
    instructions: prompt.instructions,
    input: prompt.input,
    schemaName: 'tealstreet_module_screen',
    schema: SCREEN_SCHEMA as unknown as Record<string, unknown>,
    // The judgement is the product here, not the latency.
    effort: 'medium',
    cacheKey: prompt.cacheKey,
    metadata: { feature: 'tealstreet_module_screen' },
  })) as RawVerdict | null;

  if (!raw || typeof raw !== 'object' || (raw.verdict !== 'allow' && raw.verdict !== 'block')) {
    // Fail closed: an unreadable verdict is not an approval.
    throw new AiProviderError('The safety screen returned an unreadable verdict');
  }

  const concerns = parseConcerns(raw.concerns);
  // Belt and braces: a concern the model marked `block` blocks, even if it
  // then said "allow". The two disagreeing is itself a reason not to trust it.
  const blockedByConcern = concerns.some((c) => c.severity === 'block');
  const allowed = raw.verdict === 'allow' && !blockedByConcern;
  const reason = typeof raw.reason === 'string' && raw.reason.trim() ? raw.reason.trim() : null;

  if (!allowed) {
    return {
      allowed: false,
      tier: 'blocked',
      findings,
      concerns,
      reason: reason ?? 'This module was refused by the safety screen.',
      modelSkipped: false,
    };
  }

  /**
   * The model can RAISE the tier but never lower it. A `local-only` static
   * finding stands whatever the model thinks, so nothing written in the source
   * can argue a module back into the syncing tier — the same ordering
   * invariant that governs blocks, applied to the network.
   */
  const tier: ScreenTier = concerns.some((c) => c.severity === 'local-only') ? 'local-only' : staticTier;

  return {
    allowed: true,
    tier,
    findings,
    concerns,
    // A local-only module is allowed, but the author still needs to be told
    // why it will not sync — silence here reads as a bug on their other device.
    reason:
      tier === 'local-only'
        ? 'This module reaches the network or loads external code, so it runs only on this device and is not backed up or synced.'
        : null,
    modelSkipped: false,
  };
};
