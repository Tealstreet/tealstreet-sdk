import type { VenueFamily } from './venues.mjs';
import type { EngineModule } from './loader.mjs';

export type WebEngineModule = EngineModule & { default(module?: unknown): Promise<unknown> };
/** Shared across default and explicit browser loaders; failed initialization may retry. */
export function initializeWebEngine(family: VenueFamily, importer: () => Promise<WebEngineModule>): Promise<WebEngineModule>;
