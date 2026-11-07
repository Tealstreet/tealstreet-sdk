const urls = (scheme: 'https' | 'wss', mode: 'production' | 'simulator', path = '') => ({
  livenet: `${scheme}://${mode}.hyrotrader.invalid${path}`,
  testnet: `${scheme}://testnet.${mode}.hyrotrader.invalid${path}`,
});

export const configuredHyrotraderEndpoints = {
  HYROTRADER_PROD_URL: urls('https', 'production'),
  HYROTRADER_PROD_WS_URL: {
    public: { linear: urls('wss', 'production', '/v5/public/linear') },
    private: urls('wss', 'production', '/v5/private'),
  },
  HYROTRADER_SIMULATOR_URL: urls('https', 'simulator'),
  HYROTRADER_SIMULATOR_WS_URL: {
    public: { linear: urls('wss', 'simulator', '/v5/public/linear') },
    private: urls('wss', 'simulator', '/v5/private'),
  },
};
