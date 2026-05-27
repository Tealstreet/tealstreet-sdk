import React from 'react';
import styles from './HomepageFeatures.module.css';

const FeatureList = [
  {
    title: 'About',
    Svg: require('../../static/img/about.svg').default,
    location: '/docs/about/introduction',
    description: (
      <>
        Tealstreet is a free web and mobile trading terminal providing a powerfully consistent experience and a
        relentless edge.
      </>
    ),
  },
  {
    title: 'Connect',
    Svg: require('../../static/img/connect.svg').default,
    location: 'docs/connect/api-general',
    description: <>Quickly connect your API keys and start trading on your favorite exchange.</>,
  },
  {
    title: 'Terminal',
    Svg: require('../../static/img/trade.svg').default,
    location: '/docs/trade/terminal',
    description: (
      <>
        Tealstreet makes placing orders, managing positions and all other day-to-day operations more efficient and helps
        to enhance your trading edge.
      </>
    ),
  },
  {
    title: 'CLI',
    Svg: require('../../static/img/cli.svg').default,
    location: '/cli/',
    description: (
      <>
        A keyboard-first command interface for trading. Place orders, build chains, run TWAPs, and route TradingView
        webhooks through your own terminal.
      </>
    ),
  },
  {
    title: 'Support',
    Svg: require('../../static/img/support.svg').default,
    location: 'docs/support/discord',
    description: <>Get 24/7 support on our community discord. Click here to learn how to join.</>,
  },
];

function Feature({ Svg, title, description, location }) {
  return (
    <div className={styles.featureCol}>
      <a href={location}>
        <div className={styles.card}>
          <div className="text--center">
            <Svg className={styles.featureSvg} alt={title} />
          </div>
          <div className="text--center padding-horiz--md">
            <h3>{title}</h3>
            <p>{description}</p>
          </div>
        </div>
      </a>
    </div>
  );
}

export default function HomepageFeatures() {
  return (
    <section className={styles.features}>
      <div className="container">
        <div className="row">
          {FeatureList.map((props, idx) => (
            <Feature key={idx} {...props} />
          ))}
        </div>
      </div>
    </section>
  );
}
