/**
 * Where safe-cex reports errors and product events. Nothing is sent unless the host
 * registers a sink with `setTelemetrySink`; the SDK never ships telemetry itself.
 */
export interface TelemetryContext {
  tags?: Record<string, unknown>;
  extra?: Record<string, unknown>;
}

export interface TelemetrySink {
  event?(name: string, data: Record<string, unknown>): void;
  captureException?(error: unknown, context: TelemetryContext): void;
}

let sink: TelemetrySink = {};

export const setTelemetrySink = (next: TelemetrySink): void => {
  sink = next;
};

export const reportEvent = (name: string, data: Record<string, unknown>): void => {
  sink.event?.(name, data);
};

export const reportException = (error: unknown, context: TelemetryContext): void => {
  sink.captureException?.(error, context);
};
