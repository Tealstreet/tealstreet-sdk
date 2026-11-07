# Page candle source self-heal and bounded history reads

**Change.** A worker mirror's chart history comes from a credential-free public
`RustEngineExchange` it creates on the page (`fetchPageCandles`). This applies
to every venue without `fetchOHLCVInWorker`. That source was created once and
reused for the account's lifetime.

- **Bounds:** its reads wait at most 10 s for markets and give the candle
  command a 20 s deadline, so one history request settles in about 30 s, well
  under the chart's 60 s hung-history verdict. A stuck request fails visibly
  and `getBars` retries it.
- **Self-heal:** if a read fails because the source is broken (failed to start,
  dead engine, disposed, or still no markets a minute after it was built), the
  source is disposed. A source whose markets are only slow is kept, so the next
  read is served by it once they land. After a disposal, a later read builds a
  fresh source after a backoff that starts at 2 s and doubles to 1 min, so a
  persistently failing venue is never spun. Reads inside the backoff
  fail at once with "candle source is recovering".

**Why.** A BingX user's charts did not recover after forced resubscribes on two
symbols, reported as "unsubscribed" (hung history). That fits one page candle
source whose startup never completed: every history load waited out the 30 s
markets wait across `getBars` retries, and the reused source never recovered.

**Callers.**
- SDK: `RustEngineExchange.command` accepts a per-command `timeoutMs` (passed
  only when set).
- NX: none.

**Obligation.** None. **Deletion prerequisites.** None added.
