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
      <div className="om-landing__phone" aria-hidden="true">
        <div className="om-landing__device">
          <span className="om-landing__bar" />
          <span className="om-landing__msg om-landing__msg--in" />
          <span className="om-landing__msg om-landing__msg--out" />
          <span className="om-landing__msg om-landing__msg--in om-landing__msg--short" />
          <span className="om-landing__msg om-landing__msg--out om-landing__msg--short" />
        </div>
        <span className="om-landing__float om-landing__float--a" />
        <span className="om-landing__float om-landing__float--b" />
        <span className="om-landing__float om-landing__float--c" />
      </div>

      <div className="om-landing__wrap">
        <header className="om-landing__topbar">
          <p className="om-landing__brand">{c.brand} <span className="om-landing__beta">{c.beta}</span></p>
          <p className="om-landing__by">{c.seedlife.label}</p>
        </header>

        <div className="om-landing__hero">
          <div>
            <h1 className="om-landing__headline">{c.headline}</h1>
            <p className="om-landing__lead">{c.lead}</p>
            <div className="om-landing__cta">
              <button type="button" className="om-landing__button" data-primary="true" onClick={onGetStarted}>{c.getStarted}</button>
            </div>
            <p className="om-landing__seedlife">{c.seedlife.tagline}</p>
          </div>
          <div className="om-landing__chat" role="img" aria-label={c.chat.label}>
            {c.chat.mine.map((m) => <p key={m} className="om-landing__bubble om-landing__bubble--mine">{m}</p>)}
            <p className="om-landing__bubble om-landing__bubble--other">{c.chat.other}</p>
          </div>
        </div>

        <section aria-labelledby="om-landing-what-h">
          <h2 id="om-landing-what-h">{c.what.heading}</h2>
          <ul className="om-landing__cards">
            {c.what.cards.map((card) => <li key={card.title}><h3>{card.title}</h3><p>{card.body}</p></li>)}
          </ul>
        </section>

        <section aria-labelledby="om-landing-why-h">
          <p className="om-landing__eyebrow">{c.why.eyebrow}</p>
          <h2 id="om-landing-why-h">{c.why.heading}</h2>
          <ul className="om-landing__cards om-landing__cards--meanings">
            {c.why.items.map((it) => (
              <li key={it.words[0]}>
                <h3>{it.words.map((w, i) => <span key={w}>{i > 0 ? " " : ""}<b>{w.slice(0, 1)}</b>{w.slice(1)}</span>)}</h3>
                <p>{it.body}</p>
              </li>
            ))}
          </ul>
        </section>

        <section aria-labelledby="om-landing-yours-h">
          <p className="om-landing__eyebrow">{c.yours.eyebrow}</p>
          <div className="om-landing__split">
            <div>
              <h2 id="om-landing-yours-h">{c.yours.heading}</h2>
              {c.yours.body.map((p) => <p key={p}>{p}</p>)}
            </div>
            <blockquote>{c.yours.quote}</blockquote>
          </div>
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

        <section aria-labelledby="om-landing-security-h" id="security">
          <p className="om-landing__eyebrow">{c.security.eyebrow}</p>
          <h2 id="om-landing-security-h">{c.security.heading}</h2>
          <p>{c.security.intro}</p>
          <ul>{c.security.points.map((p) => <li key={p}>{p}</li>)}</ul>
          <p><a href={c.security.link} rel="noopener noreferrer">{c.security.linkLabel}</a></p>
        </section>

        <footer className="om-landing__footer">
          <p><a href={c.footer.seedlifeUrl} rel="noopener noreferrer">{c.footer.seedlife}</a>. {c.seedlife.tagline}</p>
        </footer>
      </div>
    </main>
  );
}
