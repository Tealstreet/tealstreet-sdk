/**
 * Direct-first WebSocket host selection with a sticky fallback CHAIN.
 *
 * Shared by the Bybit and OKX socket classes; the Bitget classes carry the same
 * counters inline (they predate this helper and were left alone). A connection
 * that produced ZERO messages is a silent failure. Once `maxSilentConnects`
 * CONSECUTIVE connections have been silent, the socket advances to the next
 * candidate (relay through the private fleet, then the athena passthrough —
 * whatever the caller lists) and stays there; if that candidate is silent for
 * another streak it advances again, and sticks at the last one. Any message at
 * all — a pong, a subscribe ack, even an auth error — proves the current path
 * works and resets the streak, so a wrong API key never trips it.
 *
 * Sticky for the socket's lifetime (a reload starts over), and every user on a
 * fallback shares its egress against the venue's per-IP caps — which is why
 * these must stay fallbacks and never become the default path.
 *
 * The host DATA lives in routing/routingTable.ts (`relay` / `fallback` on a WS
 * channel, attached by the resolver only when the channel resolved DIRECT);
 * this owns only the trigger state.
 */
export class WsDirectFallback {
  private connects = 0;
  private messages = 0;
  private silentConnects = 0;
  private activeIndex = 0; // 0 = primary, k = candidates[k - 1]

  constructor(private readonly maxSilentConnects = 2) {}

  /**
   * Call exactly once per connect attempt. `fallbackUrls` is the chain to walk
   * in order; `undefined` entries are skipped so callers can pass gated
   * candidates without branching.
   */
  resolveUrl(primaryUrl: string, ...fallbackUrls: Array<string | undefined>): string {
    const candidates = fallbackUrls.filter((url): url is string => typeof url === 'string' && url.length > 0);
    if (this.connects > 0) {
      this.silentConnects = this.messages === 0 ? this.silentConnects + 1 : 0;
    }
    this.connects++;
    this.messages = 0;
    if (this.silentConnects >= this.maxSilentConnects && this.activeIndex < candidates.length) {
      this.activeIndex++;
      this.silentConnects = 0;
    }
    return this.activeIndex === 0 ? primaryUrl : (candidates[this.activeIndex - 1] ?? primaryUrl);
  }

  /** Call from the socket's message handler. */
  onMessage = (): void => {
    this.messages++;
  };
}
