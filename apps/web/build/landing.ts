import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import type { Plugin } from "vite";
import { Landing } from "../src/landing/Landing.tsx";

export const LANDING_MARKER = "<!--om-landing-->";

/** Prerenders the landing page (the same component the app shows) into index.html so it exists without JavaScript. */
export const landingPlugin = (): Plugin => ({
  name: "om-prerender-landing",
  transformIndexHtml: {
    order: "pre",
    handler: (html) => html.replace(LANDING_MARKER, renderToStaticMarkup(createElement(Landing))),
  },
});
