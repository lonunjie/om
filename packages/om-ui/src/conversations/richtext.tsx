import { Fragment } from "react";
import type { ReactNode } from "react";
import { VisuallyHidden } from "../shell/primitives/VisuallyHidden";
import type { ConversationCopy } from "./copy";
import "./conversations.css";

/** RichTextV1 view model. Semantic nodes only: there is no HTML, style or script anywhere in the format. */
export type RichInline =
  | { readonly type: "text"; readonly text: string }
  | { readonly type: "emphasis" | "strong"; readonly children: readonly RichInline[] }
  | { readonly type: "code"; readonly text: string }
  | { readonly type: "link"; readonly destination: string; readonly children: readonly RichInline[] }
  | { readonly type: "mention"; readonly personId?: string; readonly label: string }
  | { readonly type: "attachment"; readonly name?: string };
export type RichBlock =
  | { readonly type: "paragraph"; readonly children: readonly RichInline[] }
  | { readonly type: "list"; readonly ordered?: boolean; readonly items: readonly (readonly (RichInline | RichBlock)[])[] };
export interface RichTextV1 { readonly version: 1; readonly blocks: readonly RichBlock[] }

export const plainTextToRichText = (text: string): RichTextV1 => ({
  version: 1,
  blocks: text.split(/\n{2,}/).map((p): RichBlock => ({ type: "paragraph", children: [{ type: "text", text: p }] })),
});

const MAX_DEPTH = 16;
const MAX_NODES = 2000;
const SAFE_PROTOCOLS: ReadonlySet<string> = new Set(["https:", "http:"]);

/** Only plain http(s) destinations without embedded credentials become anchors; anything else stays inert text. */
export function safeDestination(dest: unknown): string | null {
  if (typeof dest !== "string" || dest.length > 2048) return null;
  try {
    const u = new URL(dest);
    return SAFE_PROTOCOLS.has(u.protocol) && u.username === "" && u.password === "" ? u.href : null;
  } catch {
    return null;
  }
}

type Rec = Readonly<Record<string, unknown>>;
const isRec = (v: unknown): v is Rec => typeof v === "object" && v !== null && !Array.isArray(v);
const str = (v: unknown): string => (typeof v === "string" ? v : "");

interface Budget { nodes: number }

function renderNodes(v: unknown, depth: number, copy: ConversationCopy["richText"], b: Budget): ReactNode {
  if (!Array.isArray(v)) return null;
  return (v as readonly unknown[]).map((n, i) => <Fragment key={i}>{renderNode(n, depth, copy, b)}</Fragment>);
}

function renderNode(node: unknown, depth: number, copy: ConversationCopy["richText"], budget: Budget): ReactNode {
  if (depth > MAX_DEPTH || ++budget.nodes > MAX_NODES) return <span className="om-richtext__truncated">{copy.truncated}</span>;
  if (typeof node === "string") return node;
  if (!isRec(node)) return null;
  const kids = (): ReactNode => renderNodes(node["children"], depth + 1, copy, budget);
  switch (node["type"]) {
    case "text": return str(node["text"]);
    case "paragraph": return <p className="om-richtext__p">{kids()}</p>;
    case "emphasis": return <em>{kids()}</em>;
    case "strong": return <strong>{kids()}</strong>;
    case "code": return <code className="om-richtext__code">{str(node["text"])}</code>;
    case "mention": return <span className="om-richtext__mention">@{str(node["label"])}</span>;
    case "attachment": return <span className="om-richtext__attachment" data-inert="true">{copy.attachment}{str(node["name"]) === "" ? "" : ` ${str(node["name"])}`}</span>;
    case "link": {
      const dest = safeDestination(node["destination"]);
      if (dest === null) return <span className="om-richtext__unsafe-link">{kids()} <span>{copy.unsafeLink}</span></span>;
      return (
        <span className="om-richtext__link">
          <a href={dest} rel="noopener noreferrer" target="_blank" referrerPolicy="no-referrer">{kids()}<VisuallyHidden> {copy.opensExternally}</VisuallyHidden></a>
          {" "}<span className="om-richtext__destination">({dest})</span>
        </span>
      );
    }
    case "list": {
      const items = Array.isArray(node["items"]) ? (node["items"] as readonly unknown[]) : [];
      const Tag = node["ordered"] === true ? "ol" : "ul";
      return <Tag className="om-richtext__list">{items.map((it, i) => <li key={i}>{renderNodes(it, depth + 1, copy, budget)}</li>)}</Tag>;
    }
    default: {
      // Unknown node: show its text if it has any; never throw, never interpret.
      const t = str(node["text"]);
      return <span className="om-richtext__unknown">{t === "" ? copy.unsupported : t}</span>;
    }
  }
}

/** Renders untrusted RichTextV1 as React elements. Malformed or hostile input degrades to inert text. */
export function RichText({ doc, copy }: { readonly doc: unknown; readonly copy: ConversationCopy["richText"] }): ReactNode {
  const blocks = isRec(doc) && Array.isArray(doc["blocks"]) ? (doc["blocks"] as readonly unknown[]) : null;
  const budget: Budget = { nodes: 0 };
  if (blocks === null) return <span className="om-richtext__unknown">{copy.unsupported}</span>;
  return <div className="om-richtext">{renderNodes(blocks, 0, copy, budget)}</div>;
}
