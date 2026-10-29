// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { CopyNotice } from "./CopyNotice";
import { copyNoticeCopy } from "./copy/en";
import { isCopyOrigin } from "./official";

afterEach(cleanup);

describe("isCopyOrigin", () => {
  it("is false for the official site, its Pages project and previews, and local runs", () => {
    for (const o of ["https://om.lonunjie.org", "https://om-lonunjie.pages.dev", "https://beta-preview.om-lonunjie.pages.dev", "http://localhost:5173", "http://127.0.0.1:4173", "http://[::1]:8080", "http://app.localhost", "", "not a url"]) {
      expect(isCopyOrigin(o), o).toBe(false);
    }
  });
  it("is true for anyone else's deployment, including look-alike hosts", () => {
    for (const o of ["https://my-om.pages.dev", "https://om.example.com", "https://evil-om.lonunjie.org.example.net", "https://notom-lonunjie.pages.dev", "https://someone.github.io"]) {
      expect(isCopyOrigin(o), o).toBe(true);
    }
  });
});

describe("CopyNotice", () => {
  it("links to the official site with rel=noopener on a copy", () => {
    render(<CopyNotice origin="https://om.example.com" />);
    const a = screen.getByRole("link", { name: copyNoticeCopy.linkLabel });
    expect(a.getAttribute("href")).toBe("https://om.lonunjie.org");
    expect(a.getAttribute("rel")).toContain("noopener");
    expect(screen.getByRole("note").textContent).toContain("self-hosted copy of OM");
  });
  it("renders nothing on the official site or locally", () => {
    const { container, rerender } = render(<CopyNotice origin="https://om.lonunjie.org" />);
    expect(container.innerHTML).toBe("");
    rerender(<CopyNotice origin="http://localhost:5173" />);
    expect(container.innerHTML).toBe("");
  });
});
