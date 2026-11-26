// SPDX-License-Identifier: MPL-2.0
import { useId, useState } from "react";
import type { ReactNode } from "react";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import { SecurityNoticeBanner } from "../shell/theme/SecurityNoticeBanner";
import type { PluginsCopy } from "./copy";
import { pluginErrorCode, pluginUiLimits } from "./plugins";
import type { PluginUiIntent, PluginUiNodeView, PluginUiTreeView } from "./plugins";

type RenderNode = { readonly node: PluginUiNodeView; readonly children: readonly RenderNode[] };

/** Keep rendering bounded even if an adapter hands us an inconsistent view. Core validates the wire tree. */
function layout(nodes: readonly PluginUiNodeView[]): RenderNode | null {
  if (nodes.length > pluginUiLimits.nodes) return null;
  const byParent = new Map<string | undefined, PluginUiNodeView[]>();
  for (const node of nodes) {
    const siblings = byParent.get(node.parent);
    if (siblings === undefined) byParent.set(node.parent, [node]);
    else siblings.push(node);
  }
  const roots = byParent.get(undefined);
  if (roots?.length !== 1 || roots[0] === undefined) return null;
  const seen = new Set<string>();
  function visit(node: PluginUiNodeView, depth: number): RenderNode | null {
    if (depth > pluginUiLimits.depth || seen.has(node.id)) return null;
    seen.add(node.id);
    const children: RenderNode[] = [];
    for (const child of byParent.get(node.id) ?? []) {
      const rendered = visit(child, depth + 1);
      if (rendered === null) return null;
      children.push(rendered);
    }
    return { node, children };
  }
  const tree = visit(roots[0], 1);
  return seen.size === nodes.length ? tree : null;
}

function formFields(children: readonly RenderNode[], values: ReadonlyMap<string, string>): readonly { nodeId: string; value: string }[] {
  return children.flatMap(({ node, children: descendants }) => {
    if (node.kind === "form") return [];
    const nested = formFields(descendants, values);
    return node.kind === "input" ? [{ nodeId: node.id, value: values.get(node.id) ?? "" }, ...nested] : nested;
  });
}

/** Renders data inside a host-labelled area. Plugin values never become styling, DOM attributes or host commands. */
export function PluginUiTree({ tree, copy, unavailable, onIntent }: {
  readonly tree: PluginUiTreeView;
  readonly copy: PluginsCopy;
  readonly unavailable: string | null;
  readonly onIntent: (intent: PluginUiIntent) => void;
}) {
  const headingId = useId();
  const reasonId = useId();
  const [values, setValues] = useState<ReadonlyMap<string, string>>(() => new Map());
  const root = layout(tree.nodes);
  if (root === null) {
    return <SecurityNoticeBanner reasonCode="invalid_ui_tree" label={copy.errors[pluginErrorCode.invalidUiTree]} tone="danger" />;
  }

  function render({ node, children }: RenderNode): ReactNode {
    const label = node.text?.trim() ? node.text : undefined;
    const contents = children.map(render);
    const enabled = node.action !== undefined && unavailable === null;
    const description = unavailable === null ? undefined : reasonId;
    const missingActionId = `${reasonId}-${String(tree.nodes.indexOf(node))}`;
    const event = { kind: "ui_interaction", target: tree.id, nodeId: node.id } as const;
    switch (node.kind) {
      case "text":
        return <div key={node.id} className="om-plugin-stack">
          <Text><bdi>{node.text}</bdi></Text>{contents}
        </div>;
      case "button":
        return <div key={node.id} className="om-plugin-stack">
          <div><Button disabled={!enabled} aria-describedby={node.action === undefined ? missingActionId : description} onClick={() => {
            onIntent({ ...event, event: "activate" });
          }}><bdi>{label ?? copy.pluginButton}</bdi></Button></div>
          {node.action === undefined ? <p id={missingActionId}>{copy.pluginControlUnavailable}</p> : null}
          {contents}
        </div>;
      case "input":
        return <div key={node.id} className="om-plugin-stack">
          <label className="om-plugin-stack">
            <bdi>{label ?? copy.pluginInput}</bdi>
            <input className="om-plugin-input" type="text" value={values.get(node.id) ?? ""}
              autoComplete="off" spellCheck={false} aria-describedby={description}
              readOnly={unavailable !== null} onChange={(event) => {
                const value = event.currentTarget.value;
                setValues((previous) => new Map(previous).set(node.id, value));
              }} />
          </label>
          {contents}
        </div>;
      case "card":
      case "group":
        return <div key={node.id} className={node.kind === "card" ? "om-plugin-stack om-plugin-card" : "om-plugin-stack"}
          role={label === undefined ? undefined : "group"} aria-label={label}>
          {label === undefined ? null : <Text weight="medium"><bdi>{label}</bdi></Text>}{contents}
        </div>;
      case "form":
        return <div key={node.id} className="om-plugin-stack" role="group" aria-label={label ?? copy.pluginSubmit}>
          {label === undefined ? null : <Text weight="medium"><bdi>{label}</bdi></Text>}
          {contents}
          {node.action === undefined ? null : <div><Button disabled={!enabled} aria-describedby={description} onClick={() => {
            onIntent({ ...event, event: "submit", fields: formFields(children, values) });
          }}>{copy.pluginSubmit}</Button></div>}
        </div>;
    }
  }

  return <section className="om-plugin-stack om-plugin-tree" aria-labelledby={headingId}>
    <h3 id={headingId}>{copy.pluginControls}</h3>
    {unavailable === null ? null : <p id={reasonId}>{unavailable}</p>}
    {render(root)}
  </section>;
}
