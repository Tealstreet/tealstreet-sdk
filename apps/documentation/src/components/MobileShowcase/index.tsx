import React, { useCallback, useEffect, useRef, useState } from 'react';
import Link from '@docusaurus/Link';
import useBaseUrl from '@docusaurus/useBaseUrl';
import clsx from 'clsx';

import styles from './styles.module.css';

/** Clips are real simulator recordings, encoded as `<name>.mp4`, `<name>.webm` and a `<name>.jpg` poster. */
const CLIP_DIR = '/video/mobile';

type Feature = {
  id: string;
  title: string;
  blurb: string;
  clips: { name: string; label: string }[];
};

const FEATURES: Feature[] = [
  {
    id: 'trade',
    title: 'Trade',
    blurb:
      'Market and limit orders in two taps. Drag TP and SL straight off your position on the chart, then swipe to close.',
    clips: [
      { name: 'trade-order', label: 'Placing a 0.2 SOL market buy' },
      { name: 'trade-tpsl', label: 'Dragging take profit and stop loss off the position line' },
      { name: 'trade-close', label: 'Swiping the position left and tapping Close' },
    ],
  },
  {
    id: 'markets',
    title: 'Markets',
    blurb: 'Every market on your exchange, with live prices and 24h change. Filter by type or search by symbol.',
    clips: [{ name: 'markets', label: 'Filtering and searching the Market tab' }],
  },
  {
    id: 'portfolio',
    title: 'Portfolio',
    blurb: 'Holdings, margin and exposure across your accounts, with positions and open orders underneath.',
    clips: [{ name: 'portfolio', label: 'The Portfolio tab with a position and its stops' }],
  },
  {
    id: 'news',
    title: 'News',
    blurb: 'A live feed of traders and headlines, with a one-tap Buy and Sell bar for the coin in the story.',
    clips: [{ name: 'news', label: 'Scrolling the live Traders feed' }],
  },
  {
    id: 'alerts',
    title: 'Alerts',
    blurb: 'Set a price level and get a push on your phone the moment the market crosses it.',
    clips: [{ name: 'alert', label: 'Creating a price alert on SOLUSDT' }],
  },
  {
    id: 'cloud',
    title: 'Cloud jobs',
    blurb: 'Chasers and TWAPs run on Tealstreet’s servers, so they keep working after you lock your phone.',
    clips: [{ name: 'cloud-jobs', label: 'Closing a position with a TWAP and watching it on Cloud jobs' }],
  },
];

const SETUP_STEPS: { title: string; body: React.ReactNode; clip?: string; image?: string; label: string }[] = [
  {
    title: 'Get the app',
    body: (
      <>
        Ask for an invite in our <Link href="https://teal.link/discord">Discord</Link>, then install from TestFlight or
        Firebase App Tester.
      </>
    ),
    image: '/img/trading/mobile/app/signin-top-right-icon.png',
    label: 'The app on first launch, with live market data before you sign in',
  },
  {
    title: 'Sign in',
    body: 'Tap the icon at the top-right. Sign up or sign in with email, Google, Discord, X or Apple.',
    clip: 'signin',
    label: 'Signing in from the top-right icon',
  },
  {
    title: 'Add your API key',
    body: 'Pick your exchange, paste the key and secret from your exchange, tap Done.',
    clip: 'add-key',
    label: 'Adding a Bybit API key in the app',
  },
  {
    title: 'Trade',
    body: 'Choose a size and tap Buy or Sell. Positions and orders update live.',
    clip: 'trade-order',
    label: 'Placing a market order',
  },
];

function usePrefersReducedMotion(): boolean {
  const [reduced, setReduced] = useState(false);
  useEffect(() => {
    const query = window.matchMedia('(prefers-reduced-motion: reduce)');
    setReduced(query.matches);
    const onChange = () => setReduced(query.matches);
    query.addEventListener('change', onChange);
    return () => query.removeEventListener('change', onChange);
  }, []);
  return reduced;
}

/** Tracks whether an element is near the viewport. `seen` latches so a loaded clip is not dropped on scroll. */
function useInView<T extends Element>(rootMargin = '200px') {
  const ref = useRef<T>(null);
  const [visible, setVisible] = useState(false);
  const [seen, setSeen] = useState(false);
  useEffect(() => {
    const node = ref.current;
    if (!node) return;
    const observer = new IntersectionObserver(
      ([entry]) => {
        setVisible(entry.isIntersecting);
        if (entry.isIntersecting) setSeen(true);
      },
      { rootMargin },
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, [rootMargin]);
  return { ref, visible, seen };
}

type ClipProps = {
  name: string;
  label: string;
  loop?: boolean;
  onEnded?: () => void;
  onProgress?: (fraction: number) => void;
};

/**
 * A muted, inline clip that loads only near the viewport and pauses off-screen.
 * With reduced motion it shows the poster until the reader asks to play it.
 */
export function Clip({ name, label, loop = true, onEnded, onProgress }: ClipProps) {
  const base = useBaseUrl(`${CLIP_DIR}/${name}`);
  const reducedMotion = usePrefersReducedMotion();
  const [playRequested, setPlayRequested] = useState(false);
  const { ref, visible, seen } = useInView<HTMLDivElement>();
  const videoRef = useRef<HTMLVideoElement>(null);
  const animate = !reducedMotion || playRequested;

  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;
    if (visible) void video.play().catch(() => undefined);
    else video.pause();
  }, [visible, seen, name, animate]);

  return (
    <div ref={ref} className={styles.clip}>
      {animate && seen ? (
        <video
          ref={videoRef}
          key={name}
          className={styles.media}
          poster={`${base}.jpg`}
          muted
          playsInline
          autoPlay
          loop={loop}
          preload="none"
          aria-label={label}
          controls={reducedMotion}
          onEnded={onEnded}
          onTimeUpdate={(event) => {
            const { currentTime, duration } = event.currentTarget;
            if (onProgress && duration) onProgress(currentTime / duration);
          }}
        >
          <source src={`${base}.webm`} type="video/webm" />
          <source src={`${base}.mp4`} type="video/mp4" />
        </video>
      ) : (
        <>
          <img className={styles.media} src={`${base}.jpg`} alt={label} loading="lazy" />
          {reducedMotion && (
            <button type="button" className={styles.playButton} onClick={() => setPlayRequested(true)}>
              ▶ Play clip
            </button>
          )}
        </>
      )}
    </div>
  );
}

export function Phone({ children, size = 'large' }: { children: React.ReactNode; size?: 'large' | 'small' }) {
  return (
    <div className={clsx(styles.phone, size === 'small' && styles.phoneSmall)}>
      <div className={styles.screen}>{children}</div>
    </div>
  );
}

/** One looping clip in a small centred phone, for the reference pages. */
export function MobileClip({ name, label }: { name: string; label: string }) {
  return (
    <figure className={styles.inline}>
      <Phone size="small">
        <Clip name={name} label={label} />
      </Phone>
      <figcaption className={styles.caption}>{label}</figcaption>
    </figure>
  );
}

/** Product hero: a phone playing real footage, driven by a clickable feature list. */
export function MobileHero() {
  const reducedMotion = usePrefersReducedMotion();
  const [featureIndex, setFeatureIndex] = useState(0);
  const [clipIndex, setClipIndex] = useState(0);
  const [progress, setProgress] = useState(0);
  const feature = FEATURES[featureIndex];
  const clip = feature.clips[clipIndex];

  const select = useCallback((index: number) => {
    setFeatureIndex(index);
    setClipIndex(0);
    setProgress(0);
  }, []);

  const advance = useCallback(() => {
    if (clipIndex + 1 < feature.clips.length) {
      setClipIndex(clipIndex + 1);
      setProgress(0);
    } else {
      select((featureIndex + 1) % FEATURES.length);
    }
  }, [clipIndex, feature.clips.length, featureIndex, select]);

  return (
    <section className={styles.hero}>
      <div className={styles.heroCopy}>
        <p className={styles.eyebrow}>Tealstreet for iPhone and Android</p>
        <h1 className={styles.headline}>Your whole desk, in your pocket.</h1>
        <p className={styles.lede}>
          Charts, orders, positions, news and alerts in a native app built for one-handed trading. Your chasers and
          TWAPs keep running in the cloud when the phone is locked.
        </p>
        <div className={styles.ctas}>
          <Link
            className={clsx('button button--primary button--lg', styles.cta)}
            to="/docs/trade/mobile/getting-started"
          >
            Get started
          </Link>
          <Link className={clsx('button button--secondary button--lg', styles.cta)} href="https://teal.link/discord">
            Request an invite
          </Link>
        </div>
        <ul className={styles.features} role="tablist" aria-label="App features">
          {FEATURES.map((item, index) => {
            const active = index === featureIndex;
            const filled = active ? (clipIndex + progress) / item.clips.length : 0;
            return (
              <li key={item.id} role="presentation">
                <button
                  type="button"
                  role="tab"
                  aria-selected={active}
                  className={clsx(styles.feature, active && styles.featureActive)}
                  onClick={() => select(index)}
                >
                  <span className={styles.featureTitle}>{item.title}</span>
                  <span className={styles.featureBlurb}>{item.blurb}</span>
                  {active && !reducedMotion && (
                    <span className={styles.featureProgress} style={{ transform: `scaleX(${filled})` }} />
                  )}
                </button>
              </li>
            );
          })}
        </ul>
      </div>
      <div className={styles.heroPhone}>
        <div className={styles.glow} aria-hidden />
        <Phone>
          <Clip name={clip.name} label={clip.label} loop={false} onEnded={advance} onProgress={setProgress} />
        </Phone>
        <p className={styles.caption}>{clip.label}</p>
      </div>
    </section>
  );
}

/** "Get set up in 60 seconds": each step with its own looping clip. */
export function SetupSteps() {
  const imageBase = useBaseUrl('/');
  return (
    <section className={styles.setup}>
      <h2 className={styles.setupHeading}>Get set up in 60 seconds</h2>
      <ol className={styles.steps}>
        {SETUP_STEPS.map((step, index) => (
          <li key={step.title} className={styles.step}>
            <Phone size="small">
              {step.clip ? (
                <Clip name={step.clip} label={step.label} />
              ) : (
                <img
                  className={styles.media}
                  src={`${imageBase}${step.image.slice(1)}`}
                  alt={step.label}
                  loading="lazy"
                />
              )}
            </Phone>
            <div className={styles.stepText}>
              <span className={styles.stepNumber}>{index + 1}</span>
              <h3 className={styles.stepTitle}>{step.title}</h3>
              <p className={styles.stepBody}>{step.body}</p>
            </div>
          </li>
        ))}
      </ol>
    </section>
  );
}
