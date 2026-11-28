import React from 'react';
import useBaseUrl from '@docusaurus/useBaseUrl';
import clsx from 'clsx';

import { Clip, Phone } from '../MobileShowcase';
import styles from './styles.module.css';

/**
 * A callout on the step's screenshot, in the screen's own coordinates (`space`): iPhone points for
 * phone shots, so boxes can be copied from the app's view hierarchy. `box` outlines a control;
 * a callout without w/h is a tap point. `n` is the badge shown on it.
 */
export type Callout = {
  n?: number | string;
  x: number;
  y: number;
  w?: number;
  h?: number;
  shape?: 'box' | 'tap';
  label?: string;
};

type AnnotatedStepProps = {
  n: number;
  title: string;
  /** Still under /static, e.g. /img/trading/mobile/steps/sync-unlock.jpg. */
  image?: string;
  /** Clip name under /static/video/mobile, played instead of a still. */
  clip?: string;
  alt: string;
  device?: 'phone' | 'desktop';
  callouts?: Callout[];
  /** Coordinate space of the callouts: [width, height]. Defaults to the iPhone 17 Pro's 402 × 874 points. */
  space?: [number, number];
  children: React.ReactNode;
};

const PHONE_SPACE: [number, number] = [402, 874];
const DESKTOP_SPACE: [number, number] = [960, 608];

function Overlay({ callouts, space }: { callouts: Callout[]; space: [number, number] }) {
  const [sw, sh] = space;
  return (
    <div className={styles.overlay} aria-hidden>
      {callouts.map((c, i) => {
        const tap = c.shape === 'tap' || c.w === undefined;
        return (
          <div
            key={i}
            className={clsx(styles.callout, tap ? styles.tap : styles.box)}
            style={{
              left: `${(c.x / sw) * 100}%`,
              top: `${(c.y / sh) * 100}%`,
              width: tap ? undefined : `${((c.w ?? 0) / sw) * 100}%`,
              height: tap ? undefined : `${((c.h ?? 0) / sh) * 100}%`,
            }}
          >
            {c.n !== undefined && <span className={styles.badge}>{c.n}</span>}
            {c.label && <span className={styles.label}>{c.label}</span>}
          </div>
        );
      })}
    </div>
  );
}

function DesktopFrame({ children }: { children: React.ReactNode }) {
  return (
    <div className={styles.desktop}>
      <div className={styles.desktopBar}>
        <span />
        <span />
        <span />
      </div>
      <div className={styles.desktopScreen}>{children}</div>
    </div>
  );
}

/** One numbered step: annotated screen on one side, title and caption on the other. */
export function AnnotatedStep({
  n,
  title,
  image,
  clip,
  alt,
  device = 'phone',
  callouts = [],
  space,
  children,
}: AnnotatedStepProps) {
  const src = useBaseUrl(image ?? '/');
  const media = (
    <div className={styles.media}>
      {clip ? <Clip name={clip} label={alt} /> : <img src={src} alt={alt} loading="lazy" />}
      <Overlay callouts={callouts} space={space ?? (device === 'phone' ? PHONE_SPACE : DESKTOP_SPACE)} />
    </div>
  );
  return (
    <li className={clsx(styles.step, device === 'desktop' && styles.stepDesktop)}>
      <div className={styles.visual}>
        {device === 'phone' ? <Phone size="small">{media}</Phone> : <DesktopFrame>{media}</DesktopFrame>}
      </div>
      <div className={styles.text}>
        <div className={styles.heading}>
          <span className={styles.number}>{n}</span>
          <h4 className={styles.title}>{title}</h4>
        </div>
        <div className={styles.body}>{children}</div>
      </div>
    </li>
  );
}

/** Wraps a run of AnnotatedSteps into one connected, numbered sequence. */
export function Steps({ children }: { children: React.ReactNode }) {
  return <ol className={styles.steps}>{children}</ol>;
}

export default AnnotatedStep;
