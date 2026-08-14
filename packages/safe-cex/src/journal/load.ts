import type { ExchangeName } from '../types';
import type { JournalExclusion, JournalRecord, JournalSelection, JournalSelectionOptions } from './select';

import { selectJournalRecords } from './select';
import { getJournalVenueStatus } from './venue-grading';

/**
 * Loading journal records across the user's connected accounts.
 *
 * **A FAILED FETCH IS THE FOURTH INSTANCE OF ONE FAILURE, and it gets its own state here
 * rather than being discovered later.** The same mistake has now appeared in three places:
 *
 * | Surface        | The mistake                                                  |
 * | -------------- | ------------------------------------------------------------ |
 * | audit tool     | printed `rows: 0` for an envelope no reader understood       |
 * | legacy writer  | emitted `exitPrice: 0` when it never observed a close        |
 * | journal render | an excluded venue showing as an empty tab                    |
 *
 * A network error, an expired key or a rate limit is **none of** those: it is not an absence
 * (the user may have traded plenty), and it is not an exclusion (we support the venue fine).
 * Collapsing it into either is how "your history is empty" gets shown to someone whose key
 * just expired.
 *
 * So loading returns THREE disjoint outcomes — loaded, excluded, failed — and the caller must
 * render all three. This mirrors the rule the audit path already follows: a refusal THROWS
 * rather than returning `[]`, because an error that becomes an empty array is indistinguishable
 * from an idle account.
 */

export type JournalFetchFailure = {
  readonly exchange: ExchangeName;
  readonly account: string;
  /**
   * Coarse cause, for deciding what the UI offers — a retry button, a re-auth prompt, or a
   * wait. Kept closed-set rather than free text so the surface cannot drift into rendering
   * raw venue error strings, which routinely contain symbols and payload fragments.
   */
  /**
   * `disconnected` is its own kind and NOT an error: the account exists in the user's list
   * but has no running exchange instance, so nothing was ever fetched. It is not
   * `unsupported` — we support the venue fine — and not a fetch failure, because there was
   * no fetch. Collapsing it into either is the easiest and least visible mistake available
   * here: into `unsupported` it becomes permanent when it is not, into `unknown` it offers a
   * retry that cannot help.
   */
  readonly kind: 'auth' | 'rate-limit' | 'network' | 'disconnected' | 'unknown';
  /** Safe, already-sanitized message. Never the raw error object. */
  readonly message: string;
  /** True when retrying could plausibly succeed without user action. */
  readonly retryable: boolean;
};

export type JournalLoadResult = {
  readonly records: readonly JournalRecord[];
  /** Venues we cannot show at all. See `select.ts` — an exclusion is not an absence. */
  readonly excluded: readonly JournalExclusion[];
  /**
   * Accounts whose fetch FAILED. Non-empty means the numbers on screen are INCOMPLETE, and
   * every aggregate derived from them is a lower bound rather than a total. The UI must say
   * so — a partial total presented as a total is the same lie as an empty tab.
   */
  readonly failed: readonly JournalFetchFailure[];
  /**
   * False when any account failed. A single flag so a caller cannot forget to check
   * `failed.length` before presenting a sum as complete.
   */
  readonly complete: boolean;
};

export type JournalAccountSource = {
  readonly exchange: ExchangeName;
  readonly account: string;
  /** Rejects on failure. It must NOT resolve `[]` on error — that is the bug this file exists for. */
  readonly fetch: () => Promise<readonly JournalRecord[]>;
};

/**
 * Classify without echoing the venue's raw text.
 *
 * Matching is deliberately narrow: an over-eager `auth` match on the word "key" would tell a
 * rate-limited user to re-authenticate, which is worse than `unknown`.
 */
export const classifyJournalFailure = (error: unknown): Pick<JournalFetchFailure, 'kind' | 'retryable'> => {
  const text = (error instanceof Error ? error.message : String(error ?? '')).toLowerCase();

  if (/\b(401|403|unauthorized|invalid signature|invalid api|expired)\b/.test(text)) {
    // Not retryable: retrying an expired key just fails again and hides the real fix.
    return { kind: 'auth', retryable: false };
  }
  // Checked before the generic branches: the marker is ours, so it is exact rather than a
  // guess at the venue's wording.
  if (text.includes(DISCONNECTED_MARKER)) return { kind: 'disconnected', retryable: false };
  if (/\b(429|rate limit|too many requests)\b/.test(text)) return { kind: 'rate-limit', retryable: true };
  if (/\b(timeout|timed out|econn|enotfound|network|socket)\b/.test(text)) {
    return { kind: 'network', retryable: true };
  }
  return { kind: 'unknown', retryable: true };
};

/**
 * Marker for "this account has no running exchange instance".
 *
 * A sentinel rather than a message match, because every other kind is classified by guessing
 * at a VENUE's wording and this one is entirely ours — there is no reason to make it fuzzy.
 */
export const DISCONNECTED_MARKER = '__journal_account_disconnected__';

export const disconnectedAccountError = () => new Error(DISCONNECTED_MARKER);

const MESSAGE_BY_KIND: Record<JournalFetchFailure['kind'], string> = {
  auth: 'Could not authenticate with this exchange. Check the API key for this account.',
  disconnected: 'This account is not connected right now, so its trades are not included.',
  'rate-limit': 'This exchange is rate limiting us. Trade history will load shortly.',
  network: 'Could not reach this exchange. Trade history for this account is missing.',
  unknown: 'Trade history for this account could not be loaded.',
};

export const loadJournalRecords = async (
  sources: readonly JournalAccountSource[],
  options: JournalSelectionOptions = {},
): Promise<JournalLoadResult> => {
  const failed: JournalFetchFailure[] = [];
  const loaded: JournalRecord[] = [];

  // An unsupported venue is never FETCHED. Asking a venue we cannot map costs a request and
  // can only produce a failure that would then be reported as one — turning a known
  // exclusion into a spurious error.
  const fetchable = sources.filter((source) => getJournalVenueStatus(source.exchange).grade !== 'unsupported');

  const settled = await Promise.allSettled(fetchable.map((source) => source.fetch()));

  settled.forEach((outcome, index) => {
    const source = fetchable[index];
    if (outcome.status === 'fulfilled') {
      loaded.push(...outcome.value);
      return;
    }
    const { kind, retryable } = classifyJournalFailure(outcome.reason);
    failed.push({
      exchange: source.exchange,
      account: source.account,
      kind,
      retryable,
      message: MESSAGE_BY_KIND[kind],
    });
  });

  // Selection still sees every source as a known account, so an unsupported venue is reported
  // as an exclusion even though it was never fetched.
  const selection: JournalSelection = selectJournalRecords(loaded, {
    ...options,
    knownAccounts: [
      ...(options.knownAccounts ?? []),
      ...sources.map((s) => ({ exchange: s.exchange, account: s.account })),
    ],
  });

  return {
    records: selection.records,
    excluded: selection.excluded,
    failed,
    complete: failed.length === 0,
  };
};
