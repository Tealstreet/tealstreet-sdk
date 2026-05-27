import { IGrvtConfig, EGrvtEnvironment } from '../config/config';
import { getCookieWithExpiration } from '../utils/cookie';
import axios, { AxiosInstance } from 'axios';

interface GrvtCookie {
  gravity: string;
  expires: number;
  XGrvtAccountId?: string;
}

export class GrvtBaseClient {
  protected config: IGrvtConfig;
  protected cookie: GrvtCookie | null = null;
  protected domain: string;
  protected edgeBaseUrl: string;
  protected axios: AxiosInstance;
  protected edgeAxios: AxiosInstance;
  protected useRelativeEdgeUrls: boolean;

  constructor(config: IGrvtConfig) {
    this.config = config;
    this.domain = this.getDomain();
    this.edgeBaseUrl = `https://edge.${this.domain}`;
    this.axios = config.customAxios || axios.create();
    // When customEdgeAxios is provided, use relative URLs so axios respects the baseURL (for proxy support)
    this.edgeAxios = config.customEdgeAxios || axios.create();
    this.useRelativeEdgeUrls = !!config.customEdgeAxios;
  }

  protected shouldRefreshCookie(): boolean {
    if (!this.config.apiKey) {
      return false;
    }

    const timeTillExpiration = this.cookie ? this.cookie.expires - Date.now() / 1000 : null;
    const isCookieFresh = timeTillExpiration !== null && timeTillExpiration > 5;

    if (!isCookieFresh) {
      console.info(
        `Cookie should be refreshed, time till expiration: ${timeTillExpiration} seconds`
      );
    }

    return !isCookieFresh;
  }

  protected async refreshCookie(): Promise<GrvtCookie | null> {
    if (!this.shouldRefreshCookie()) {
      return this.cookie;
    }

    // When customEdgeAxios is provided, use relative URL so axios respects the baseURL (for proxy support)
    const path = this.useRelativeEdgeUrls ? '/auth/api_key/login' : this.edgeBaseUrl + '/auth/api_key/login';
    this.cookie = await getCookieWithExpiration(path, this.config.apiKey || null, this.edgeAxios);

    if (this.cookie) {
      console.info('Cookie refreshed');
    }

    return this.cookie;
  }

  protected async authenticatedGet<RequestParams = Record<string, any>, ResponseData = any>(
    endpoint: string,
    params?: RequestParams
  ): Promise<ResponseData> {
    try {
      await this.refreshCookie();
      const { axios: axiosInstance, path } = this.getAxiosAndPath(endpoint);
      const response = await axiosInstance.get(path, {
        params,
        headers: this.getAuthenticatedHeaders(),
      });
      return response.data as ResponseData;
    } catch (error) {
      throw new Error(error instanceof Error ? error.message : 'Unknown error');
    }
  }

  protected async get<RequestParams = Record<string, any>, ResponseData = any>(
    endpoint: string,
    params?: RequestParams
  ): Promise<ResponseData> {
    try {
      const { axios: axiosInstance, path } = this.getAxiosAndPath(endpoint);
      const response = await axiosInstance.get(path, {
        params,
        headers: this.getHeaders(),
      });
      return response.data as ResponseData;
    } catch (error) {
      throw new Error(error instanceof Error ? error.message : 'Unknown error');
    }
  }

  protected async authenticatedPost<RequestData = Record<string, any>, ResponseData = any>(
    endpoint: string,
    payload: RequestData
  ): Promise<ResponseData> {
    try {
      await this.refreshCookie();
      const { axios: axiosInstance, path } = this.getAxiosAndPath(endpoint);
      const response = await axiosInstance.post(path, payload, {
        headers: this.getAuthenticatedHeaders(),
      });
      return response.data as ResponseData;
    } catch (error) {
      throw new Error(error instanceof Error ? error.message : 'Unknown error');
    }
  }

  private getHeaders(): Record<string, string> {
    return {
      'Content-Type': 'application/json',
    };
  }

  private getAuthenticatedHeaders(): Record<string, string> {
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
    };

    if (this.cookie) {
      headers['Cookie'] = `gravity=${this.cookie.gravity}`;
      if (this.cookie.XGrvtAccountId) {
        headers['X-Grvt-Account-Id'] = this.cookie.XGrvtAccountId;
      }
    }

    return headers;
  }

  private getDomain(): string {
    switch (this.config.env) {
      case EGrvtEnvironment.PRODUCTION:
        return 'grvt.io';
      case EGrvtEnvironment.TESTNET:
        return 'testnet.grvt.io';
      case EGrvtEnvironment.STAGING:
        return 'staging.gravitymarkets.io';
      case EGrvtEnvironment.DEV:
        return 'dev.gravitymarkets.io';
      default:
        throw new Error(`Unknown environment: ${this.config.env}`);
    }
  }

  /**
   * Get the axios instance and path to use for a given endpoint.
   * When customEdgeAxios is provided, Edge endpoints use relative URLs.
   */
  protected getAxiosAndPath(endpoint: string): { axios: AxiosInstance; path: string } {
    const isEdgeEndpoint = endpoint.startsWith(this.edgeBaseUrl);
    if (isEdgeEndpoint && this.useRelativeEdgeUrls) {
      // Strip the edgeBaseUrl to get relative path
      const relativePath = endpoint.substring(this.edgeBaseUrl.length);
      return { axios: this.edgeAxios, path: relativePath };
    }
    return { axios: this.axios, path: endpoint };
  }
}
