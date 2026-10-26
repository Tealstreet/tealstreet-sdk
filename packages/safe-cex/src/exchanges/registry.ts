import { createRustRegistry } from './rustRegistry';

export const exchangeRegistry = createRustRegistry(() =>
  import('./rust/rust.exchange').then((module) => module.RustEngineExchange),
);

export { getRegisteredExchangeNames } from './rustRegistry';
