import * as setCookieParser from 'set-cookie-parser';
import axios, { AxiosInstance } from 'axios';

interface GrvtCookie {
  gravity: string;
  expires: number;
  XGrvtAccountId?: string;
}

export async function getCookieWithExpiration(
  path: string,
  apiKey: string | null,
  axiosInstance?: AxiosInstance
): Promise<GrvtCookie | null> {
  if (!apiKey) {
    return null;
  }

  const client = axiosInstance || axios.create();

  try {
    const response = await client.post(path, { api_key: apiKey }, {
      headers: {
        'Content-Type': 'application/json',
      },
    });

    if (response.status !== 200) {
      console.warn(`Invalid response for cookie: ${response.status} ${response.statusText}`);
      return null;
    }

    // Handle both array and string formats for set-cookie header
    // (array from native HTTP, string from HTTP-over-WebSocket tunnel)
    let cookieHeader: string | undefined;
    const setCookieRaw = response.headers['set-cookie'] || response.headers['Set-Cookie'];
    if (Array.isArray(setCookieRaw)) {
      cookieHeader = setCookieRaw[0];
    } else if (typeof setCookieRaw === 'string') {
      cookieHeader = setCookieRaw;
    }

    const accountId = response.headers['x-grvt-account-id'] || response.headers['X-Grvt-Account-Id'];

    if (!cookieHeader) {
      console.warn('No cookie header in response');
      return null;
    }

    // Parse the cookie header using set-cookie-parser
    const cookies = setCookieParser.parse(cookieHeader);
    const gravityCookie = cookies.find(
      (cookie: setCookieParser.Cookie) => cookie.name === 'gravity'
    );

    if (!gravityCookie) {
      console.warn('No gravity cookie in response');
      return null;
    }

    if (!gravityCookie.expires) {
      console.warn('No expiration date in cookie');
      return null;
    }

    return {
      gravity: gravityCookie.value,
      expires: gravityCookie.expires.getTime() / 1000, // Convert to Unix timestamp
      XGrvtAccountId: accountId || undefined,
    };
  } catch (error) {
    console.error('Error getting cookie:', error);
    return null;
  }
}
