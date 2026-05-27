import React from 'react';
import clsx from 'clsx';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import styles from './index.module.css';
import HomepageFeatures from '../components/HomepageFeatures';
import ReactGridLayout from '../components/GridLayout';
import Terminalcard from '../components/terminalcard';
import Translate, { translate } from '@docusaurus/Translate';

function HomepageHeader() {
  const { siteConfig } = useDocusaurusContext();
  return (
    <header className={clsx('hero hero--primary', styles.heroBanner)}>
      <div className="container">
        <div className={styles.buttons}>
          <Link className="button button--secondary button--lg" to="/blog/Trading-Competition">
            Click Here for More Information
          </Link>
        </div>
      </div>
    </header>
  );
}

export default function Home() {
  const { siteConfig } = useDocusaurusContext();
  return (
    <Layout title={`Homepage | ${siteConfig.title}`} description="Tealstreet Documentation">
      {/*<HomepageHeader/>*/}
      <main>
        <Terminalcard />
        <HomepageFeatures />
      </main>
    </Layout>
  );
}
