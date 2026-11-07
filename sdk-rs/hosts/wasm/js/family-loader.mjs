import { venueFamily } from './venues.mjs';

/**
 * Loads a venue's family engine once per family. wasm-bindgen initialization
 * replaces module-wide memory, so concurrent and later accounts share one
 * pending or completed initialization; a failure lets the next load retry.
 * `resolveFamily` maps a venue to its family and throws for a venue this loader
 * must not serve, before any import.
 */
export function familyLoader(importFamily, resolveFamily = venueFamily) {
  const initialized = new Map();
  return async (venue) => {
    const family = resolveFamily(venue);
    let loading = initialized.get(family);
    if (!loading) {
      loading = Promise.resolve(family)
        .then(importFamily)
        .then(async (engine) => {
          await engine.default();
          return engine;
        });
      initialized.set(family, loading);
      loading.catch(() => {
        if (initialized.get(family) === loading) initialized.delete(family);
      });
    }
    return loading;
  };
}
