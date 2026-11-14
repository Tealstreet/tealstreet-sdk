import type { DelegatedIntent } from '../delegatedOutbox';

// Checked by the normal package compiler; never executed by the runtime suite.
export function notSentRequiresEvidence(row: DelegatedIntent): void {
  row.state = 'unknown';
  row.error = { message: 'uncertain', outcome: 'unknown' };
  // @ts-expect-error Working control: unrelated state words are also refused.
  row.state = 'invented';
  // @ts-expect-error Only the evidence constructor can construct notSent.
  row.state = 'notSent';
  // @ts-expect-error Error outcomes cannot fabricate no-handoff evidence either.
  row.error = { message: 'unproven', outcome: 'notSent' };
}
