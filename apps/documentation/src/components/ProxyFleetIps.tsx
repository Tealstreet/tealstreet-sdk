import React, { useState } from 'react';

import { legacyWhitelistVenues } from '../../../../packages/proxy-ips-lists/src/legacyWhitelist';
import { whitelistIpGroups } from '../../../../packages/proxy-ips-lists/src/whitelist';

/** Same display-only sources and copy policy as the web account form. */
export default function ProxyFleetIps({ exchange }: { exchange?: string }) {
  const [chosenExchange, setExchange] = useState('bybit');
  const venue = exchange ?? chosenExchange;
  const groups = whitelistIpGroups(venue);
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
      <p>Current Tealstreet fleet</p>
      {list(groups.fleet, 'Tealstreet proxy fleet IP addresses')}
      {groups.extras.length > 0 && (
        <>
          <p>Additional venue addresses</p>
          {list(groups.extras, 'Additional venue IP addresses')}
        </>
      )}
    </div>
  );
}
