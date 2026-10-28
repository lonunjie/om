import type { ReactNode } from "react";
import { landingCopy as c } from "../copy/en";

export interface LandingProps {
  /** Wired in the running app. In the prerendered page nothing is wired: the button waits for the app to load. */
  readonly onGetStarted?: () => void;
  /** Dynamic install control (the prerendered page has none, only the static instructions). */
  readonly installSlot?: ReactNode;
}

/**
 * The landing page. It uses plain elements and no imports with side effects, because the build renders it to static
 * HTML inside index.html (see build/landing.ts); styling is landing.css, loaded by the entry point.
 */
export function Landing({ onGetStarted, installSlot }: LandingProps) {
  return (
    <main className="om-landing" id="om-landing">
      <header className="om-landing__hero">
        <p className="om-landing__brand">{c.brand}</p>
        <h1 className="om-landing__headline">{c.headline}</h1>
        <p className="om-landing__lead">{c.lead}</p>
        <div className="om-landing__cta">
          <button type="button" className="om-landing__button" data-primary="true" onClick={onGetStarted}>{c.getStarted}</button>
        </div>
        <p className="om-landing__seedlife"><strong>{c.seedlife.label}.</strong> {c.seedlife.tagline}</p>
      </header>

      <section className="om-landing__status" aria-labelledby="om-landing-status-h">
        <h2 id="om-landing-status-h">{c.status.heading}</h2>
        <p>{c.status.body}</p>
      </section>

      <section aria-labelledby="om-landing-what-h">
        <h2 id="om-landing-what-h">{c.what.heading}</h2>
        <ul>{c.what.points.map((p) => <li key={p}>{p}</li>)}</ul>
      </section>

      <section aria-labelledby="om-landing-install-h" id="install">
        <h2 id="om-landing-install-h">{c.install.heading}</h2>
        <p>{c.install.intro}</p>
        <ul>
          <li>{c.install.ios}</li>
          <li>{c.install.android}</li>
          <li>{c.install.desktop}</li>
        </ul>
        {installSlot}
      </section>

      <section aria-labelledby="om-landing-self-h" id="run-your-own">
        <h2 id="om-landing-self-h">{c.selfHost.heading}</h2>
        <p>{c.selfHost.body}</p>
        <p><a href={c.selfHost.link} rel="noopener noreferrer">{c.selfHost.linkLabel}</a></p>
      </section>

      <footer className="om-landing__footer">
        <p><a href={c.footer.seedlifeUrl} rel="noopener noreferrer">{c.footer.seedlife}</a>. {c.seedlife.tagline}</p>
      </footer>
    </main>
  );
}
