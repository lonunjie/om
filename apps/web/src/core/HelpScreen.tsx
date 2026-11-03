// Settings > Help and feedback: where to report a problem or suggest an idea. Both go to GitHub issues, which are public, so the page
// says so and says what never to paste. The only thing added to a report is the version number, as a visible part of the link.
import { Icon, Text, type IconName } from "@om/ui";
import { helpCopy as c } from "../copy/en";

declare const __OM_VERSION__: string | undefined;
export const appVersion = (): string => (typeof __OM_VERSION__ === "string" ? __OM_VERSION__ : "dev");

/** A report link that opens the form with the version already filled in. */
export const withVersion = (url: string, version: string): string => {
  const u = new URL(url);
  if (u.searchParams.has("template")) u.searchParams.set("version", version);
  return u.toString();
};

export function HelpScreen({ version = appVersion() }: { readonly version?: string }) {
  const rows: readonly { readonly icon: IconName; readonly row: { readonly title: string; readonly hint: string; readonly url: string } }[] = [
    { icon: "settings", row: c.actions.problem },
    { icon: "plus", row: c.actions.idea },
    { icon: "search", row: c.actions.browse },
    { icon: "shield", row: c.actions.security },
  ];
  return (
    <section className="om-lock" aria-labelledby="om-help-h">
      <Text as="h1" size="xl" weight="bold" id="om-help-h">{c.heading}</Text>
      <Text as="p">{c.intro}</Text>
      <ul className="om-menu__list om-help__list">
        {rows.map(({ icon, row }) => (
          <li key={row.url}>
            <a className="om-menu__row" href={withVersion(row.url, version)} target="_blank" rel="noopener noreferrer">
              <span className="om-menu__icon" aria-hidden="true"><Icon name={icon} size={22} /></span>
              <span className="om-menu__text"><span className="om-menu__title">{row.title}</span><span className="om-menu__hint">{row.hint} {c.newTab}</span></span>
              <Icon name="chevron" size={20} />
            </a>
          </li>
        ))}
      </ul>
      <Text as="p" size="sm" role="note">{c.privacy}</Text>
      <Text as="p" size="sm" tone="secondary">{c.version(version)}</Text>
    </section>
  );
}
