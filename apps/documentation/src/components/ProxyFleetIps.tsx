import React, { useState } from 'react';

import { docsWhitelistIpGroups, proxyFleetWhitelistVenues } from './proxyFleetIpData';

/** Display-only whitelist snapshot for the docs site's self-contained Vercel build. */
export default function ProxyFleetIps({ exchange }: { exchange?: string }) {
  const [chosenExchange, setExchange] = useState('bybit');
  const venue = exchange ?? chosenExchange;
  const groups = docsWhitelistIpGroups(venue);
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
            {proxyFleetWhitelistVenues.map((name) => (
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
