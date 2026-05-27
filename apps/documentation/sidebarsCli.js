// @ts-check

/** @type {import('@docusaurus/plugin-content-docs').SidebarsConfig} */
const sidebars = {
  cliSidebar: [
    'intro',
    'install',
    'quickstart',
    {
      type: 'category',
      label: 'Recipes',
      collapsed: false,
      items: [
        'recipes/bracket-order',
        'recipes/chaser',
        'recipes/twap',
        'recipes/scale-ladder',
        'recipes/trailing-stop',
        'recipes/fan-out-symbols',
        'recipes/listener-from-web',
        'recipes/webhook-from-tradingview',
      ],
    },
    {
      type: 'category',
      label: 'Reference',
      collapsed: false,
      items: [
        'reference/commands',
        'reference/variables',
        'reference/aliases-and-vars',
        'reference/sizing-and-prices',
        'reference/workflow',
        'reference/safety',
        'reference/output',
        'reference/web-cli',
      ],
    },
    {
      type: 'category',
      label: 'Operations',
      collapsed: true,
      items: [
        'operations/listener',
        'operations/hooks-and-webhooks',
        'operations/trusted-clients',
        'operations/pairing',
        'operations/recording',
        'operations/troubleshooting',
        'operations/config-files',
        'operations/releases',
      ],
    },
  ],
};

module.exports = sidebars;
