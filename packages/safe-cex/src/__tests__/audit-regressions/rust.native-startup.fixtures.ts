import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { Store } from '../../store/store.interface';
import type { StoreData } from '../../types';

import { vi } from 'vitest';

import { consumerFrame } from '../cutover/consumerFrame';

/**
 * Command-wrapper fakes still publish the native datasets that authorize startup.
 * After start, a frame is published whenever the native overrides (e.g. loaded flags) change.
 */
export function installNativeStartupFrames(
  engine: WasmEngineLike,
  store: Store,
  native: () => Partial<StoreData> = () => ({}),
): void {
  let started = false;
  let published: string | undefined;
  const start = engine.start.bind(engine);
  engine.start = vi.fn((now: number) => {
    started = true;
    return start(now);
  });
  engine.takeFrame = (_now, marketOnly) => {
    if (!started || marketOnly) return undefined;
    const overrides = structuredClone(native());
    const key = JSON.stringify({ loaded: store.loaded, ...overrides });
    if (key === published) return undefined;
    published = key;
    return new Uint8Array(consumerFrame({ ...store.serialize(), ...overrides }));
  };
  engine.snapshotFrame = () => new Uint8Array(consumerFrame({ ...store.serialize(), ...structuredClone(native()) }));
}
