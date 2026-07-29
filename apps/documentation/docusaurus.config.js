// @ts-check
// Note: type annotations allow type checking and IDEs autocompletion

import { themes } from 'prism-react-renderer';
import versions from './versions.json';
// import VersionsArchived from './versionsArchived.json';
const lightCodeTheme = themes.github; // light theme
const darkCodeTheme = themes.dracula; // dark theme
const defaultLocale = 'en';

// function isPrerelease(version: string) {
// }

function getLastVersion() {
  const firstStableVersion = versions.find((version) => !version.includes('2.0.0') && !version.includes('3.0.0'));
  return firstStableVersion;
}

/** @type {import('@docusaurus/types').Config} */
const config = {
  title: 'Tealstreet Docs',
  tagline: 'Built with love <3',
  url: 'https://docs.tealstreet.io', //your website url
  baseUrl: '/', //Base url for the project
  onBrokenLinks: 'warn',
  onDuplicateRoutes: 'warn',
  onBrokenAnchors: 'warn',
  favicon: 'img/_favicon.ico',
  organizationName: 'tealstreetio', // Usually your GitHub org/user name.
  projectName: 'tealstreet-docs', // Usually your repo name.
  future: {
    faster: {
      swcJsLoader: true,
      swcJsMinimizer: true,
      swcHtmlMinimizer: false,
      lightningCssMinimizer: true,
      rspackBundler: true,
      mdxCrossCompilerCache: false,
    },
  },

  plugins: [
    require.resolve('docusaurus-plugin-image-zoom'),
    [
      require.resolve('@easyops-cn/docusaurus-search-local'),
      {
        hashed: true,
        language: [defaultLocale],
        indexDocs: true,
        indexBlog: false,
        indexPages: false,
        highlightSearchTermsOnTargetPage: true,
        docsRouteBasePath: ['/docs', '/cli'],
      },
    ],
    [
      '@docusaurus/plugin-content-docs',
      {
        id: 'cli',
        path: 'cli',
        routeBasePath: 'cli',
        sidebarPath: require.resolve('./sidebarsCli.js'),
        sidebarCollapsible: true,
        showLastUpdateAuthor: false,
        showLastUpdateTime: false,
        exclude: ['**/CLAUDE.md', '**/TODO.md', '**/*.private.md'],
      },
    ],
    [
      '@docusaurus/plugin-client-redirects',
      {
        redirects: [
          {
            from: '/docs/connect/bybitv5',
            to: '/docs/2.0.0/connect/bybitv5',
          },
          {
            from: '/docs/connect/mexc',
            to: '/docs/2.0.0/connect/mexc',
          },
          {
            from: '/docs/connect/api-encryption',
            to: '/docs/2.0.0/connect/api-encryption',
          },
          {
            from: '/docs/connect/okex',
            to: '/docs/connect/okx',
          },
          // CLI section was promoted to its own /cli/ route in 2026.
          // Old /docs/cli/* URLs (linked from elsewhere on the web) keep
          // working via these redirects.
          { from: '/docs/cli', to: '/cli/' },
          { from: '/docs/cli/quickstart', to: '/cli/quickstart' },
          { from: '/docs/cli/commands', to: '/cli/reference/commands' },
          { from: '/docs/cli/concepts', to: '/cli/' },
          { from: '/docs/cli/cookbook', to: '/cli/quickstart' },
          { from: '/docs/cli/listener', to: '/cli/operations/listener' },
          { from: '/docs/cli/pairing', to: '/cli/operations/pairing' },
          { from: '/docs/cli/recording', to: '/cli/operations/recording' },
          { from: '/docs/cli/releases', to: '/cli/operations/releases' },
          { from: '/docs/cli/troubleshooting', to: '/cli/operations/troubleshooting' },
          { from: '/docs/cli/vars-and-aliases', to: '/cli/reference/aliases-and-vars' },
        ],
      },
    ],
  ],

  presets: [
    [
      '@docusaurus/preset-classic',
      /** @type {import('@docusaurus/preset-classic').Options} */
      {
        docs: {
          showLastUpdateAuthor: false,
          showLastUpdateTime: false,
          sidebarPath: require.resolve('./sidebars.js'),
          sidebarCollapsible: true,
          disableVersioning: false,
          lastVersion: 'current',
          editCurrentVersion: true,
          // Keep internal-only siblings (CLAUDE.md, TODO.md, *.private.md)
          // co-located with their public counterparts but never shipped.
          exclude: ['**/CLAUDE.md', '**/TODO.md', '**/*.private.md'],

          // addtl info for versions, custom banners etc: https://github.com/facebook/docusaurus/discussions/7112#discussioncomment-7146770
          versions: {
            current: {
              label: 'V3',
              banner: 'none',
              badge: false,
            },
            '2.0.0': {
              label: 'V2',
              path: '2.0.0',
              banner: 'unmaintained',
              badge: true,
            },
          },
        },
        blog: {
          showReadingTime: true,
          // Please change this to your repo.
          // editUrl: 'https://github.com/tealstreetio/tealstreet-docs/',
        },
        theme: {
          customCss: [require.resolve('./src/css/custom.css'), require.resolve('./src/css/mobile.css')],
        },
        svgr: {
          svgrConfig: {
            // Silence SVGO v4 preset-default override warnings in docs builds.
            svgo: false,
          },
        },
        googleAnalytics: {
          trackingID: 'UA-197923509-4',
          anonymizeIP: true,
        },
      },
    ],
  ],

  themeConfig:
    /** @type {import('@docusaurus/preset-classic').ThemeConfig} */
    ({
      colorMode: {
        defaultMode: 'dark',
        respectPrefersColorScheme: false,
      },
      // announcementBar: {
      //   id: 'support_us',
      //   content:
      //     '⭐️ The Bybit <b>WSOT</b> trading tournament is active you can <a target="_blank" rel="noopener noreferrer" href="https://teal.link/wsot2024">join here</a> ⭐️',
      //   backgroundColor: '#005000',
      //   textColor: '#ffffff',
      //   isCloseable: true,
      // },
      navbar: {
        title: '',
        logo: {
          alt: 'Tealstreet Logo',
          src: 'img/tealstreet-logo-light.png',
          srcDark: 'img/tealstreet-logo-dark.png',
          target: '_self',
        },
        hideOnScroll: true,
        items: [
          // left side
          {
            label: 'About',
            position: 'left',
            to: 'docs/about/introduction',
          },
          {
            label: 'Connect',
            position: 'left',
            to: 'docs/connect/api-general',
          },
          {
            label: 'Terminal',
            position: 'left',
            to: 'docs/trade/terminal',
          },
          {
            label: 'CLI',
            position: 'left',
            to: 'cli/',
          },
          // {
          //   label: "Webhooks",
          //   position: "left",
          //   to: "docs/2.0.0/webhooks",
          // },
          {
            label: 'Referrals',
            position: 'left',
            to: 'docs/ref-links',
          },
          {
            label: 'V3',
            position: 'left',
            to: 'docs/about/version-3',
          },
          // right side
          {
            label: 'FAQ',
            position: 'right',
            to: 'docs/FAQ',
          },
          {
            label: 'Support',
            position: 'right',
            to: 'docs/support/contact-us',
          },
          // {
          //   label: 'Changelog',
          //   position: 'right',
          //   to: 'docs/changelog',
          // },
          // {
          //   href: 'https://github.com/tealstreet/',
          //   position: 'right',
          //   className: 'header-github-link',
          //   'aria-label': 'GitHub repository',
          // },
          {
            type: 'docsVersionDropdown',
            position: 'right',
          },
          {
            //Translation Submodule
            type: 'localeDropdown',
            position: 'right',
          },
        ],
      },
      prism: {
        theme: lightCodeTheme,
        darkTheme: darkCodeTheme,
        additionalLanguages: ['powershell', 'bash'],
      },
      zoom: {
        selector: '.markdown :not(em) > img',
        background: {
          light: 'rgb(249, 249, 249)',
          dark: 'rgb(7, 29, 33)',
        },
        config: {
          // options you can specify via https://github.com/francoischalifour/medium-zoom#usage
          margin: 24,
          scrollOffset: 0,
        },
      },
      footer: {
        logo: {
          alt: 'Tealstreet Logo',
          src: 'img/tealstreet-logo-dark.png',
          href: 'https://tealstreet.io',
        },
        style: 'dark',
        links: [
          {
            title: 'Documentation',
            items: [
              {
                label: 'About',
                to: '/docs/about/introduction',
              },
              {
                label: 'Connect',
                to: '/docs/connect/api-general',
              },
              {
                label: 'Terminal',
                to: '/docs/trade/terminal',
              },
              {
                label: 'CLI',
                to: '/cli/',
              },
            ],
          },
          {
            title: 'Community',
            items: [
              {
                label: 'Discord',
                href: 'https://teal.link/discord',
              },
              {
                label: 'Twitter',
                href: 'https://twitter.com/tealstreet',
              },
              {
                label: 'Youtube',
                href: 'https://www.youtube.com/channel/UC9MlPSRRlLWc9v4u8Idxi-w',
              },
              {
                label: 'GitHub',
                href: 'https://github.com/tealstreetio/',
              },
              {
                label: 'Twitch',
                href: 'https://www.twitch.tv/tradersamwise',
              },
              {
                label: 'Feature Request',
                to: '/docs/requests/feature-requests',
              },
            ],
          },
          {
            title: 'Legal',
            items: [
              {
                label: 'Terms of Service',
                //to: '/docs/TOS/terms-of-service',
                href: 'https://www.tealstreet.io/terms-of-service',
              },
              {
                label: 'Risk Warning',
                //to: '/docs/TOS/risk-warning',
                href: 'https://www.tealstreet.io/risk-warning/',
              },
              {
                label: 'Privacy Policy',
                //to: '/docs/TOS/privacy-policy',
                href: 'https://www.tealstreet.io/privacy-policy/',
              },
              {
                label: 'User Declaration',
                //to: '/docs/TOS/user-declaration',
                href: 'https://www.tealstreet.io/user-declaration/',
              },
            ],
          },
          {
            title: 'Support Tealstreet',
            items: [
              {
                label: 'Binance',
                href: 'https://teal.link/Binance1',
              },
              {
                label: 'Bitget',
                href: 'https://teal.link/Bitget',
              },
              {
                label: 'Bybit',
                href: 'https://teal.link/bybit1',
              },
              {
                label: 'BitMEX',
                href: 'https://teal.link/BitMEX',
              },
              {
                label: 'BloFin',
                href: 'https://teal.link/blofin',
              },
              {
                label: 'ApeX',
                href: 'https://teal.link/apex',
              },
              {
                label: 'Backpack',
                href: 'https://teal.link/backpack',
              },
              {
                label: 'Coinbase',
                href: 'https://teal.link/coinbase',
              },
            ],
          },
          {
            title: ' ',
            items: [
              {
                label: 'OKX',
                href: 'https://teal.link/OKX',
              },
              {
                label: 'Phemex',
                href: 'https://teal.link/Phemex',
              },
              {
                label: 'BingX',
                href: 'https://teal.link/bingx20',
              },
              {
                label: 'WOOFi',
                href: 'https://dex.woo.org/',
              },
              {
                label: 'WOOX',
                href: 'https://teal.link/woo',
              },
              {
                label: 'Bitrue',
                href: 'https://teal.link/bitrue',
              },
              {
                label: 'Breakout',
                href: 'https://teal.link/breakout',
              },
              {
                label: 'Hyperliquid',
                href: 'https://teal.link/hyperliquid',
              },
              // {
              //   label: 'Kucoin',
              //   href: 'https://teal.link/kucoin',
              // },
              // {
              //   label: 'Gate.io',
              //   href: 'https://teal.link/gateio',
              // },
            ],
          },
          {
            title: ' ',
            items: [
              {
                label: 'AsterDex',
                href: 'https://teal.link/aster5',
              },
              {
                label: 'Lighter',
                href: 'https://teal.link/lighter',
              },
              {
                label: 'Mode',
                href: 'https://teal.link/mode',
              },
              {
                label: 'What',
                href: 'https://teal.link/whatdex',
              },
              {
                label: 'Paradex',
                href: 'https://teal.link/paradex',
              },
              {
                label: 'KodiakFI',
                href: 'https://teal.link/kodiak',
              },
              {
                label: 'Bitunix',
                href: 'https://teal.link/bitunix',
              },
              {
                label: 'GRVT',
                href: 'https://teal.link/grvt',
              },
              // {
              //   label: 'Kucoin',
              //   href: 'https://teal.link/kucoin',
              // },
              // {
              //   label: 'Gate.io',
              //   href: 'https://teal.link/gateio',
              // },
            ],
          },
        ],
        copyright: `Copyright © ${new Date().getFullYear()} Tealstreet.io`,
      },
    }),
  // This section is for the guts of the translation submodule
  i18n: {
    defaultLocale: 'en',
    // locales: ['en', 'zh-CN', 'pl', 'hu', 'pt', 'tr', 'es'],
    locales: ['en', 'zh-CN'],
    path: 'i18n',
    localeConfigs: {
      en: {
        label: 'English',
        direction: 'ltr',
        htmlLang: 'en-US',
        calendar: 'gregory',
        path: 'en',
      },
      // de: {
      //   label: 'Deutsch',
      //   direction: 'ltr',
      //   htmlLang: 'de-DE',
      //   path: 'de',
      // },
      'zh-CN': {
        label: '简体中文',
        direction: 'ltr',
        htmlLang: 'zh-CN',
        calendar: 'gregory',
        path: 'zh-cn',
      },
      // pl: {
      //   label: 'Polski',
      //   direction: 'ltr',
      //   htmlLang: 'pl-PL',
      //   calendar: 'gregory',
      //   path: 'pl',
      // },
      // hu: {
      //   label: 'Magyar',
      //   direction: 'ltr',
      //   htmlLang: 'hu-HU',
      //   calendar: 'gregory',
      //   path: 'hu',
      // },
      // pt: {
      //   label: 'Português',
      //   direction: 'ltr',
      //   htmlLang: 'pt-PT',
      //   calendar: 'gregory',
      //   path: 'pt',
      // },
      // tr: {
      //   label: 'Türkçe',
      //   direction: 'ltr',
      //   htmlLang: 'tr-TR',
      //   calendar: 'gregory',
      //   path: 'tr',
      // },
      // es: {
      //   label: 'Español',
      //   direction: 'ltr',
      //   htmlLang: 'es-ES',
      //   calendar: 'gregory',
      //   path: 'es',
      // },
    },
  },
  markdown: {
    format: 'detect',
    hooks: {
      onBrokenMarkdownLinks: 'warn',
    },
  },
};

module.exports = config;
