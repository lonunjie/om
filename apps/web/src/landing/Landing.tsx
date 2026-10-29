import type { ReactNode } from "react";
import { landingCopy as c } from "../copy/en";

export interface LandingProps {
  /** Wired in the running app. In the prerendered page nothing is wired: the button waits for the app to load. */
  readonly onGetStarted?: () => void;
  /** Dynamic install control (the prerendered page has none, only the static instructions). */
  readonly installSlot?: ReactNode;
}

/** Marks emphasis in copy: **bold**, and [[blue]] for the words the page leans on. They can nest, as in **we are [[one]]**. */
const em = (text: string): ReactNode[] => text.split(/(\[\[.+?\]\])/).map((part, i) => (part.startsWith("[[") ? <strong key={i} className="om-landing__em">{part.slice(2, -2)}</strong> : part));
const rich = (text: string): ReactNode[] => text.split("**").map((part, i) => (i % 2 === 1 ? <strong key={i}>{em(part)}</strong> : em(part)));

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
          <p className="om-landing__by"><a href={c.byline.url} rel="noopener noreferrer">{c.byline.label}</a></p>
        </header>

        <div className="om-landing__hero">
          <div>
            <h1 className="om-landing__headline">{c.headline}</h1>
            <p className="om-landing__lead">{c.lead}</p>
            <div className="om-landing__cta">
              <button type="button" className="om-landing__button" data-primary="true" onClick={onGetStarted}>
                {c.getStarted}<span className="om-landing__arrow" aria-hidden="true">{"\u2192"}</span>
              </button>
            </div>
          </div>
          <div className="om-landing__chat" role="img" aria-label={c.chat.label}>
            {c.chat.incoming.map((m) => <p key={m} className="om-landing__bubble om-landing__bubble--other">{m}</p>)}
            <p className="om-landing__bubble om-landing__bubble--mine">{c.chat.reply}</p>
          </div>
        </div>

        <section aria-labelledby="om-landing-why-h">
          <h2 id="om-landing-why-h">{c.why.heading}</h2>
          <p className="om-landing__intro">{rich(c.why.intro)}</p>
          <ul className="om-landing__cards">
            {c.why.points.map((pt) => <li key={pt.title}><h3>{pt.title}</h3><p>{pt.body}</p></li>)}
          </ul>
          <p className="om-landing__intro om-landing__closing">{rich(c.why.closing)}</p>
        </section>

        <section aria-labelledby="om-landing-names-h">
          <h2 id="om-landing-names-h">{c.names.heading}</h2>
          <p className="om-landing__intro">{c.names.intro}</p>
          <ul className="om-landing__cards om-landing__cards--meanings">
            {c.names.items.map((it) => (
              <li key={it.title}>
                <h3>{it.title.split("*").map((part, i) => (i % 2 === 1 ? <b key={part}>{part}</b> : part))}</h3>
                <p>{it.body}{it.link ? <>{" "}<a href={it.link.url} rel="noopener noreferrer">{it.link.label}</a></> : null}</p>
              </li>
            ))}
          </ul>
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
          <div className="om-landing__cta">
            <button type="button" className="om-landing__button" onClick={onGetStarted}>
              {c.startUsing}<span className="om-landing__arrow" aria-hidden="true">{"\u2192"}</span>
            </button>
          </div>
          <div className="om-landing__own" id="run-your-own">
            <h3 id="om-landing-self-h">{c.selfHost.heading}</h3>
            <p>{c.selfHost.body}</p>
            <p><a href={c.selfHost.link} rel="noopener noreferrer">{c.selfHost.linkLabel}</a></p>
          </div>
        </section>

        <section aria-labelledby="om-landing-security-h" id="security">
          <h2 id="om-landing-security-h">{c.security.heading}</h2>
          <p>{c.security.intro}</p>
          <ul>{c.security.points.map((p) => <li key={p}>{p}</li>)}</ul>
          <ul className="om-landing__links">{c.security.links.map((l) => <li key={l.url}><a href={l.url} rel="noopener noreferrer">{l.label}</a></li>)}</ul>
        </section>

        <section aria-labelledby="om-landing-follow-h">
          <h2 id="om-landing-follow-h">{c.follow.heading}</h2>
          <ul className="om-landing__follow">
            {c.follow.links.map((l) => (
              <li key={l.url}>
                <a href={l.url} rel="noopener noreferrer">
                  <svg aria-hidden="true" focusable="false"><use href={`/icons/social.svg#${l.icon}`} /></svg>{l.label}
                </a>
              </li>
            ))}
          </ul>
        </section>

        <footer className="om-landing__footer">
          <p>{c.footer.prefix}<a href={c.footer.seedlifeUrl} rel="noopener noreferrer">{c.footer.seedlife}</a>{c.footer.suffix}</p>
          <p><a href={c.footer.siteSource.url} rel="noopener noreferrer">{c.footer.siteSource.label}</a></p>
        </footer>
      </div>
    </main>
  );
}
