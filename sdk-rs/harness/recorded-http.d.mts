import type { Transport } from '../hosts/wasm/js/host.mjs';

/** Immediate fake route for canned replies; never a production transport fallback. */
export declare const prepareRecordedHttp: NonNullable<Transport['prepareHttp']>;
