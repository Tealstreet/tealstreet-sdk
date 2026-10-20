import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { Store } from '../../store/store.interface';

import { vi } from 'vitest';

import { consumerFrame } from '../cutover/consumerFrame';

/** Command-wrapper fakes still publish the native datasets that authorize startup. */
export function installNativeStartupFrames(engine: WasmEngineLike, store: Store): void {
  let pending = false;
  const start = engine.start.bind(engine);
  engine.start = vi.fn((now: number) => {
    pending = true;
    return start(now);
  });
  engine.takeFrame = (_now, marketOnly) => {
    if (!pending || marketOnly) return undefined;
    pending = false;
    return new Uint8Array(consumerFrame(store.serialize()));
  };
  engine.snapshotFrame = () => new Uint8Array(consumerFrame(store.serialize()));
}
