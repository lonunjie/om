// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { asProfileId } from "@om/platform-api";
import type { HomeSummary } from "@om/platform-api";
import { createHarness } from "@om/core-harness";
import { testCopy } from "./__fixtures__/copy";
import { HomeFacts } from "./HomeFacts";

afterEach(cleanup);

describe("HomeFacts in Settings (always open)", () => {
  const open = (configured: boolean, home?: HomeSummary) => {
    const harness = createHarness({ env: {}, clock: () => 1000 });
    const ctx = harness.seedProfile({ profileId: asProfileId("me"), label: "me", ...(home === undefined ? {} : { home }) });
    return render(<HomeFacts client={harness} ctx={ctx} copy={testCopy.home} alwaysOpen configured={configured} />);
  };

  it("loads the tier, observer, migration and origin facts at once, with no toggle", async () => {
    open(false);
    expect(screen.queryByRole("button")).toBeNull();
    expect(await screen.findByText("Home tier T1")).toBeTruthy();
    expect(screen.getByText("Observer: host_sees_ciphertext")).toBeTruthy();
    expect(screen.getByText("You can move your Home.")).toBeTruthy();
    expect(screen.getByText("Origin: default_origin")).toBeTruthy();
  });

  it("leaves out \"no delivery service picked\" when one is already in use", async () => {
    const none: HomeSummary = { tier: "none", observers: ["host_sees_ciphertext"], migrationAvailable: false, clientOriginChoice: ["default_origin"] };
    const { unmount } = open(true, none);
    expect(await screen.findByText("Observer: host_sees_ciphertext")).toBeTruthy();
    expect(screen.queryByText("Home tier none")).toBeNull();
    unmount();
    open(false, none);
    expect(await screen.findByText("Home tier none")).toBeTruthy();
  });
});
