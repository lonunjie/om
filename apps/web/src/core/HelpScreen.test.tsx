// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import axe from "axe-core";
import { afterEach, describe, expect, it } from "vitest";
import { helpCopy } from "../copy/en";
import { HelpScreen, withVersion } from "./HelpScreen";

afterEach(cleanup);

describe("Settings > Help and feedback", () => {
  it("offers a problem report, an idea (in Discussions), the list and a private security route, all on GitHub and opened in a new tab", async () => {
    const { container } = render(<HelpScreen version="9.9.9" />);
    const links = screen.getAllByRole("link");
    expect(links).toHaveLength(4);
    for (const a of links) {
      const url = new URL(a.getAttribute("href") ?? "");
      expect(url.origin).toBe("https://github.com");
      expect(url.pathname.startsWith("/lonunjie/om/")).toBe(true);
      expect(a.getAttribute("target")).toBe("_blank");
      expect(a.getAttribute("rel")).toBe("noopener noreferrer");
    }
    expect(screen.getByRole("link", { name: new RegExp(helpCopy.actions.problem.title) }).getAttribute("href")).toBe("https://github.com/lonunjie/om/issues/new?template=bug.yml&version=9.9.9");
    expect(screen.getByText(helpCopy.privacy)).toBeTruthy();
    expect(screen.getByText(helpCopy.version("9.9.9"))).toBeTruthy();
    const violations = (await axe.run(container, { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } })).violations;
    expect(violations.map((v) => v.id)).toEqual([]);
  });

  it("adds the version only to the report forms, and nothing else about the person", () => {
    expect(withVersion("https://github.com/lonunjie/om/issues", "1.2.3")).toBe("https://github.com/lonunjie/om/issues");
    expect(withVersion("https://github.com/lonunjie/om/security/advisories/new", "1.2.3")).toBe("https://github.com/lonunjie/om/security/advisories/new");
    expect(withVersion("https://github.com/lonunjie/om/issues/new?template=bug.yml", "1.2.3")).toBe("https://github.com/lonunjie/om/issues/new?template=bug.yml&version=1.2.3");
    expect(withVersion("https://github.com/lonunjie/om/discussions/new?category=ideas", "1.2.3")).toBe("https://github.com/lonunjie/om/discussions/new?category=ideas");
  });
});
