import { copyNoticeCopy as c } from "./copy/en";
import { isCopyOrigin } from "./official";

/** A small, honest pointer to the official site, shown only on copies. Renders nothing when prerendered (no `location`). */
export function CopyNotice({ origin = typeof location === "undefined" ? "" : location.origin }: { readonly origin?: string }) {
  if (origin === "" || !isCopyOrigin(origin)) return null;
  return <p className="om-copy-notice" role="note">{c.text}<a href={c.url} rel="noopener noreferrer">{c.linkLabel}</a>.</p>;
}
