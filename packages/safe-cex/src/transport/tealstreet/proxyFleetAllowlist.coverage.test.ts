import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import { engineRestUrls, rustRestUrls } from './engineRestUrls.test-support';
import { proxyFleetForwards } from './proxyFleetAllowlist';

describe('engine REST source coverage', () => {
  it('derives every fixed venue REST URL, including testnet/demo and auxiliary APIs', () => {
    const venues = resolve(__dirname, '../../../../../sdk-rs/venues');
    const missing = [...engineRestUrls(venues)].flatMap(([crate, urls]) =>
      urls.filter((url) => !proxyFleetForwards(url)).map((url) => `${crate}: ${url}`),
    );
    expect(missing).toEqual([]);
  });

  it('excludes documentation, sockets and test items without discarding later production code', () => {
    expect(
      rustRestUrls(`
        // See https://docs.example/ for the REST specification.
        const WS: &str = "wss://socket.example";
        #[cfg(test)] mod fixture;
        #[cfg(test)] mod tests {
          const HTTP: &str = "https://fixture.example/{id}";
        }
        enum Pending { #[cfg(test)] Fixture, Live }
        #[cfg(test)] const PAIR: (&str, &str) = ("https://fixture.example", "https://other.example");
        const REST: &str = "https://rest.example/api";
        /* const DEAD: &str = "https://retired.example"; */
        const AUX: &str = "https://aux.example/v1/markets?limit={size}";
      `),
    ).toEqual(['https://aux.example/v1/markets/', 'https://rest.example/api/']);
  });

  it('derives named and inline production/testnet substitutions from Rust branches', () => {
    expect(
      rustRestUrls(`
        let network = if options.testnet { "-testnet" } else { "" };
        let rest = format!("https://api{network}.example");
        let trade = format!("https://trades{}.example{CREATE}",
          if self.testnet { ".testnet" } else { "" });
      `),
    ).toEqual([
      'https://api-testnet.example/',
      'https://api.example/',
      'https://trades.example/',
      'https://trades.testnet.example/',
    ]);
  });

  it('requires new origin-template shapes to be handled explicitly', () => {
    expect(() => rustRestUrls('let rest = format!("https://api{network}.example");')).toThrow(
      'Unresolved REST origin template',
    );
  });
});
