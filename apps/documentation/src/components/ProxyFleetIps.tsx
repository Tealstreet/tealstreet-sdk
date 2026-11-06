import React, { useState } from 'react';

import {
  legacyWhitelistRegions,
  legacyWhitelistVenues,
} from '../../../../packages/proxy-ips-lists/src/legacyWhitelist';
import {
  LEGACY_WHITELIST_NOTE,
  OPTIONAL_LEGACY_WHITELIST_NOTE,
  whitelistIpGroups,
} from '../../../../packages/proxy-ips-lists/src/whitelist';

/** Same display-only sources and copy policy as the web account form. */
export default function ProxyFleetIps({ exchange }: { exchange?: string }) {
  const [chosenExchange, setExchange] = useState('bybit');
  const [region, setRegion] = useState<string>();
  const venue = exchange ?? chosenExchange;
  const regions = legacyWhitelistRegions(venue);
  const groups = whitelistIpGroups(venue, region);
  const list = (ips: readonly string[], label: string) => (
    <ul aria-label={label}>
      {ips.map((ip) => (
        <li key={ip}>
          <code>{ip}</code>
        </li>
      ))}
    </ul>
  );
  return (
    <div>
      {!exchange && (
        <label>
          Exchange{' '}
          <select
            value={venue}
            onChange={(event) => {
              setExchange(event.target.value);
              setRegion(undefined);
            }}
          >
            {legacyWhitelistVenues.map((name) => (
              <option key={name} value={name}>
                {name}
              </option>
            ))}
          </select>
        </label>
      )}
      {regions.length > 1 && (
        <label>
          Legacy region{' '}
          <select value={region ?? regions[0]} onChange={(event) => setRegion(event.target.value)}>
            {regions.map((name) => (
              <option key={name} value={name}>
                {name}
              </option>
            ))}
          </select>
        </label>
      )}
      <p>Current Tealstreet fleet</p>
      {list(groups.fleet, 'Tealstreet proxy fleet IP addresses')}
      {groups.legacy.length > 0 && (
        <>
          <p>{groups.legacyOptional ? OPTIONAL_LEGACY_WHITELIST_NOTE : LEGACY_WHITELIST_NOTE}</p>
          {list(groups.legacy, 'Legacy app IP addresses')}
        </>
      )}
      {groups.extras.length > 0 && (
        <>
          <p>Additional venue addresses</p>
          {list(groups.extras, 'Additional venue IP addresses')}
        </>
      )}
      {groups.legacyOptional && (
        <p>
          This venue allows {groups.limit} addresses. The web copy button includes the fleet only; legacy addresses are
          optional. A fleet-only key will not work on the legacy app.
        </p>
      )}
    </div>
  );
}
