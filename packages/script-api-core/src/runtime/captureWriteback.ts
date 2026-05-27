/**
 * `normalizeReturnForCapture` — pure §7.4 mapping. The host owns the
 * `setVar` write because `script-api-core` cannot depend on cli-core (the
 * `PreprocessStore` type lives there). This function takes whatever a
 * script handed to `api.io.return(...)` and returns a `{ primary, json? }`
 * pair the host stores into its preprocess vars.
 *
 * SPEC §7.4 table:
 *
 * | return(value) arg                       | Captured `@x` content | Sibling `@x.json` |
 * | --------------------------------------- | --------------------- | ----------------- |
 * | `string`                                | as-is                 | no                |
 * | `string[]`                              | comma-joined          | no                |
 * | `number` / `boolean`                    | stringified           | no                |
 * | `object` / nested array / anything else | JSON-stringified      | yes — also set    |
 *
 * The "string[]" branch is load-bearing for `cancel @x` interop: today's
 * grammar parses `@x` as a comma-list of order IDs, so an order-placing
 * script returns `[orderId1, orderId2]` and the host writes
 * `'orderId1,orderId2'` straight in. Don't break that table without a
 * coordinated cli-core grammar bump.
 *
 * `undefined` / `null` short-circuit: the script didn't return anything
 * meaningful, so no writeback at all. The caller checks for that case
 * via the sentinel `null` return.
 */

export interface CaptureWriteback {
  /** The string the host writes to `@<captureTo>`. */
  readonly primary: string;
  /**
   * When the input was a structured shape (object / nested array), the
   * JSON-stringified form. Host writes to `@<captureTo>.json`. Absent for
   * the scalar / string-list cases — they survive the existing grammar
   * without a sibling.
   */
  readonly json?: string;
}

const isStringArray = (value: unknown): value is readonly string[] => {
  if (!Array.isArray(value)) return false;
  for (const v of value) {
    if (typeof v !== 'string') return false;
  }
  return true;
};

/**
 * Returns `null` when the script's return value is missing or
 * unrepresentable, signaling "no capture writeback this run." Otherwise
 * returns the `{ primary, json? }` pair per the §7.4 table.
 *
 * The `null` short-circuit covers both `undefined` (script didn't call
 * `api.io.return`) and explicit `null` (script chose not to surface
 * anything). Both are no-op cases at the writeback layer.
 */
export const normalizeReturnForCapture = (value: unknown): CaptureWriteback | null => {
  if (value === undefined || value === null) return null;
  if (typeof value === 'string') {
    return { primary: value };
  }
  if (isStringArray(value)) {
    return { primary: value.join(',') };
  }
  if (typeof value === 'number' || typeof value === 'boolean') {
    return { primary: String(value) };
  }
  // Object / nested array / anything else — JSON-stringify into BOTH the
  // primary and the sibling `.json`. The script author who returns a
  // structured shape gets the JSON read out of either slot; legacy
  // `cancel @x` grammar parsing the primary still survives because
  // JSON-strings don't accidentally satisfy the comma-list pattern.
  let json: string;
  try {
    json = JSON.stringify(value);
  } catch {
    // Cyclic refs or unserializable values — surface as a placeholder so
    // the host's setVar doesn't crash. Scripts that return cyclic data
    // surface the issue via the `<unserializable>` sentinel.
    return { primary: '<unserializable>' };
  }
  return { primary: json, json };
};
