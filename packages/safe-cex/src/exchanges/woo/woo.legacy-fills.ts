/**
 * Query parameters for WOO's legacy fills route, `GET /v1/client/trades`.
 *
 * Extracted and pure because this is the half that was wrong, and it was wrong invisibly.
 * The caller is a private instance field, so no test could reach the parameter names — the
 * same reason the hedge-side bug silently dropped 42% of rows for as long as it did.
 *
 * **Every name differs from `/v3/trade/transactionHistory`, and a wrong one is ignored rather
 * than rejected.** Measured against the live account (sheet row 28), 2026-08-14:
 *
 * | intent | this route      | `/v3/trade/transactionHistory` |
 * | ------ | --------------- | ------------------------------ |
 * | page size | `size`       | `limit`                        |
 * | window    | `start_t` / `end_t` (ms) | `startTime` / `endTime` |
 * | paging    | `page` (works) | `page` ignored                |
 * | sort      | newest-first   | oldest-first                   |
 *
 * The window names are the dangerous pair. Sending `startTime`/`endTime` here returns the
 * newest page **unfiltered** — a January-2020 window came back holding August-2026 fills —
 * so the caller receives out-of-window rows and cannot tell. `limit` fails the same way,
 * quietly serving the default 100.
 *
 * `fromId` is accepted and ignored, so it is not a usable cursor; deep history goes through
 * `page` or a narrowed `start_t`/`end_t` window.
 */
export interface WooLegacyFillsQuery {
  symbol?: string;
  fromMilliseconds?: number;
  toMilliseconds?: number;
  /** Page size. Emitted as `size` — `limit` is silently ignored by this route. */
  limit: number;
  page?: number;
}

export const buildWooLegacyFillsParams = (query: WooLegacyFillsQuery): Record<string, any> => {
  const params: Record<string, any> = { size: query.limit };

  if (query.symbol) params.symbol = query.symbol;
  // Milliseconds, and only under these names. `startTime`/`endTime` are accepted-and-ignored.
  if (query.fromMilliseconds !== undefined) params.start_t = query.fromMilliseconds;
  if (query.toMilliseconds !== undefined) params.end_t = query.toMilliseconds;
  if (query.page !== undefined) params.page = query.page;

  return params;
};
