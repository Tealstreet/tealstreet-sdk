/**
 * A refusal raised by the shared adapter body before anything reaches a venue.
 *
 * It exists because `apps/web`'s `ValidationError` **logs to Grafana in its
 * constructor**. That is right for a human clicking an order button and wrong
 * for shared code: a script in a retry loop, or a mobile host, would ship
 * telemetry nobody asked for from a code path that is simply refusing bad
 * input. The web order helpers keep their own error; the portable body throws
 * this one.
 *
 * `runScript` catches and returns `{ ok: false, error }`, so the name is what
 * a script author sees.
 */
export class ScriptValidationError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'ScriptValidationError';
  }
}
