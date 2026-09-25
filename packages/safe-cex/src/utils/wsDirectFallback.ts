/**
 * Direct-first WebSocket host selection with a sticky fallback.
 *
 * Shared by the Bybit and OKX socket classes; the Bitget classes carry the same
 * counters inline (they predate this helper and were left alone). A connection
 * that produced ZERO messages is a silent failure. Once `maxSilentConnects`
 * CONSECUTIVE connections have been silent, every later connect in this
 * socket's lifetime dials the fallback host. Any message at all — a pong, a
 * subscribe ack, even an auth error — proves the direct path works, so only a
 * path that never answers trips it; a wrong API key never does.
 *
 * The fallback is sticky for the socket's lifetime (a reload starts over), and
 * every user on it shares one egress IP against the venue's per-IP caps — which
 * is why it must stay a fallback and never become the default path.
 *
 * The host DATA lives in routing/routingTable.ts (`fallback` on a WS channel,
 * attached by the resolver only when the channel resolved DIRECT); this owns
 * only the trigger state.
 */
export class WsDirectFallback {
  private connects = 0;
  private messages = 0;
  private silentConnects = 0;
  private active = false;

  constructor(private readonly maxSilentConnects = 2) {}

  /** Call exactly once per connect attempt. Returns the URL to dial. */
  resolveUrl(primaryUrl: string, fallbackUrl: string | undefined): string {
    if (this.connects > 0) {
      this.silentConnects = this.messages === 0 ? this.silentConnects + 1 : 0;
    }
    this.connects++;
    this.messages = 0;
    if (this.silentConnects >= this.maxSilentConnects) this.active = true;
    return this.active && fallbackUrl ? fallbackUrl : primaryUrl;
  }

  /** Call from the socket's message handler. */
  onMessage = (): void => {
    this.messages++;
  };
}
