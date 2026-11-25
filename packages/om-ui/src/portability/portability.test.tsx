// @vitest-environment jsdom
// SPDX-License-Identifier: MPL-2.0

import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { afterEach, describe, expect, it, vi } from "vitest";
import { testCopy as copy } from "./__fixtures__/copy";
import { Portability, PortabilityReportView } from "./index";
import type {
  ExportAnswer,
  ExportIntent,
  ImportAnswer,
  ImportIntent,
  ImportSource,
  PortabilityReport,
  PortabilityService,
  PortabilityView,
} from "./portability";

afterEach(cleanup);

const archiveReport: PortabilityReport = {
  source: "local_history",
  destination: "archive",
  details: { lossy: [{ code: "disappearing", count: 2 }] },
};
const importReport: PortabilityReport = {
  source: "archive",
  destination: "read_only",
  details: {
    preserved: [{ code: "history", count: 3 }],
    lossy: [{ code: "stale_control", count: 1 }],
    unverifiableAuthorship: [{ code: "unverified_authorship" }],
    missingMedia: [{ code: "missing_media", count: 1 }],
    timestampLimitations: [{ code: "unverified_timestamps" }],
    unsupportedFeatures: [{ code: "edits_unavailable" }],
  },
};
const source: ImportSource = {
  handle: "archive-handle",
  name: "history.zip",
  modes: { read_only: "available", same_account: "wrong_account" },
};
const contents = {
  media: "available",
  contacts: "available",
  aliases: "available",
  provenance: "available",
  local_preferences: "available",
} as const;
const view: PortabilityView = {
  formats: {
    archive: {
      history: [
        { id: "a", name: "Alex", kind: "chat" },
        { id: "a", name: "Art club", kind: "space" },
        { id: "c", name: "News", kind: "publication" },
      ],
      contents,
    },
    readable: {
      history: [{ id: "a", name: "Alex", kind: "chat" }],
      contents: {
        media: "unsupported",
        contacts: "unsupported",
        aliases: "unsupported",
        provenance: "unsupported",
        local_preferences: "unsupported",
      },
    },
  },
  import: "available",
};

function fake(overrides: Partial<PortabilityService> = {}) {
  const defaults: PortabilityService = {
    state: () => Promise.resolve(view),
    exportHistory: ({ format }) =>
      Promise.resolve({
        status: "saved",
        report: { ...archiveReport, destination: format },
      }),
    chooseImportSource: () => Promise.resolve({ status: "selected", source }),
    importHistory: () =>
      Promise.resolve({
        status: "imported",
        mode: "read_only",
        report: importReport,
      }),
  };
  const service = { ...defaults, ...overrides };
  return {
    state: vi.fn(service.state),
    exportHistory: vi.fn(service.exportHistory),
    chooseImportSource: vi.fn(service.chooseImportSource),
    importHistory: vi.fn(service.importHistory),
  };
}

async function axeViolations(el: Element) {
  const result = await axe.run(el, {
    rules: { "color-contrast": { enabled: false }, region: { enabled: false } },
  });
  return result.violations.map(
    (violation) => `${violation.id}: ${violation.help}`,
  );
}

async function setup(service = fake()) {
  const user = userEvent.setup();
  render(<Portability service={service} copy={copy} />);
  return {
    user,
    exportView: within(
      await screen.findByRole("region", { name: copy.export.title }),
    ),
    importView: within(
      await screen.findByRole("region", { name: copy.import.title }),
    ),
  };
}

async function reviewReadable(service = fake()) {
  const { user, exportView } = await setup(service);
  await user.click(
    exportView.getByRole("radio", { name: copy.format.readable }),
  );
  await user.click(exportView.getByRole("checkbox", { name: "Alex" }));
  await user.click(
    exportView.getByRole("button", { name: copy.export.review }),
  );
  return { user, exportView };
}

describe("portability", () => {
  it("appears only when the host supplies a service", () => {
    const { container } = render(<Portability copy={copy} />);
    expect(container.childElementCount).toBe(0);
  });

  it("reports loading and failure, then retries the options query on request", async () => {
    const service = fake();
    service.state.mockRejectedValueOnce(new Error("details"));
    const user = userEvent.setup();
    render(<Portability service={service} copy={copy} />);
    expect(screen.getByRole("status").textContent).toBe(copy.loading);
    expect((await screen.findByRole("alert")).textContent).toBe(
      copy.loadFailed,
    );
    await user.click(screen.getByRole("button", { name: copy.retry }));
    await screen.findByRole("region", { name: copy.export.title });
    expect(service.state).toHaveBeenCalledTimes(2);
  });

  it("disables unavailable actions and associates their reasons with the controls", async () => {
    const service = fake({
      state: () =>
        Promise.resolve({
          formats: { archive: "policy_denied", readable: "unsupported" },
          import: "device_unavailable",
        }),
    });
    const { user, exportView, importView } = await setup(service);
    const archive = exportView.getByRole<HTMLInputElement>("radio", {
      name: copy.format.archive,
      description: copy.reasons.policy_denied,
    });
    const picker = importView.getByRole<HTMLButtonElement>("button", {
      name: copy.import.chooseFile,
      description: copy.reasons.device_unavailable,
    });
    expect(archive.disabled).toBe(true);
    expect(picker.disabled).toBe(true);
    expect(
      importView.getByRole<HTMLButtonElement>("button", {
        name: copy.import.start,
        description: copy.reasons.device_unavailable,
      }).disabled,
    ).toBe(true);
    await user.click(picker);
    expect(service.chooseImportSource).not.toHaveBeenCalled();
    expect(
      await axeViolations(screen.getByRole("region", { name: copy.title })),
    ).toEqual([]);
  });

  it("reviews the selected scope before saving a plain archive and shows the returned report", async () => {
    const service = fake();
    const { user, exportView } = await setup(service);
    expect(
      exportView.getByRole<HTMLButtonElement>("button", {
        name: copy.export.review,
      }).disabled,
    ).toBe(true);
    await user.tab();
    expect(document.activeElement).toBe(
      exportView.getByRole("radio", { name: copy.format.archive }),
    );
    await user.keyboard(" ");
    await user.click(exportView.getByRole("checkbox", { name: "Art club" }));
    await user.click(exportView.getByRole("checkbox", { name: "News" }));
    fireEvent.change(exportView.getByLabelText(copy.export.from), {
      target: { value: "2026-01-01" },
    });
    fireEvent.change(exportView.getByLabelText(copy.export.to), {
      target: { value: "2026-02-01" },
    });
    for (const value of Object.values(copy.export.content)) {
      await user.click(exportView.getByRole("checkbox", { name: value }));
    }
    await user.click(
      exportView.getByRole("button", { name: copy.export.review }),
    );
    expect(service.exportHistory).not.toHaveBeenCalled();
    expect(document.activeElement).toBe(
      exportView.getByRole("heading", { name: copy.export.review }),
    );
    expect(exportView.getByText(copy.export.plaintext)).toBeTruthy();
    expect(exportView.getByText(copy.export.archiveRetention)).toBeTruthy();
    expect(exportView.queryByRole("checkbox")).toBeNull();
    expect(
      await axeViolations(
        screen.getByRole("region", { name: copy.export.title }),
      ),
    ).toEqual([]);
    await user.click(
      exportView.getByRole("button", { name: copy.export.save }),
    );
    await exportView.findByRole("heading", { name: copy.export.saved });
    const intent: ExportIntent = {
      format: "archive",
      scope: {
        history: [
          { kind: "space", id: "a" },
          { kind: "publication", id: "c" },
        ],
        from: "2026-01-01",
        to: "2026-02-01",
        contents: [
          "media",
          "contacts",
          "aliases",
          "provenance",
          "local_preferences",
        ],
      },
    };
    expect(service.exportHistory.mock.calls).toEqual([[intent]]);
    expect(
      exportView.getByText("Disappearing items left out (2)"),
    ).toBeTruthy();
  });

  it("keeps the readable-copy choice per export and asks again after going back", async () => {
    const service = fake();
    service.exportHistory.mockResolvedValueOnce({ status: "cancelled" });
    const { user, exportView } = await reviewReadable(service);
    const optIn = () =>
      exportView.getByRole<HTMLInputElement>("checkbox", {
        name: copy.export.includeDisappearing,
        description: copy.export.disappearingWarning,
      });
    expect(optIn().checked).toBe(false);
    await user.click(optIn());
    await user.click(exportView.getByRole("button", { name: copy.back }));
    expect(document.activeElement).toBe(
      exportView.getByRole("heading", { name: copy.export.scope }),
    );
    expect(
      exportView.getByRole<HTMLInputElement>("checkbox", { name: "Alex" })
        .checked,
    ).toBe(true);
    expect(
      exportView.getByRole<HTMLInputElement>("checkbox", {
        name: copy.export.content.media,
        description: copy.reasons.unsupported,
      }).disabled,
    ).toBe(true);
    await user.click(
      exportView.getByRole("button", { name: copy.export.review }),
    );
    expect(optIn().checked).toBe(false);
    await user.click(
      exportView.getByRole("button", { name: copy.export.save }),
    );
    await exportView.findByText(copy.export.cancelled);
    await user.click(optIn());
    await user.click(
      exportView.getByRole("button", { name: copy.export.save }),
    );
    await exportView.findByRole("heading", { name: copy.export.saved });
    expect(service.exportHistory.mock.calls.map(([intent]) => intent)).toEqual([
      {
        format: "readable",
        scope: {
          history: [{ id: "a", kind: "chat" }],
          from: null,
          to: null,
          contents: [],
        },
        includeDisappearing: false,
      },
      {
        format: "readable",
        scope: {
          history: [{ id: "a", kind: "chat" }],
          from: null,
          to: null,
          contents: [],
        },
        includeDisappearing: true,
      },
    ]);
    await user.click(
      exportView.getByRole("button", { name: copy.export.again }),
    );
    expect(
      exportView.getByRole<HTMLInputElement>("radio", {
        name: copy.format.readable,
      }).checked,
    ).toBe(false);
  });

  it("announces a slow save, prevents duplicate submission and allows retry after a refusal", async () => {
    let complete: (answer: ExportAnswer) => void = () => {
      throw new Error("no export pending");
    };
    const service = fake({
      exportHistory: () =>
        new Promise((resolve) => {
          complete = resolve;
        }),
    });
    const { user, exportView } = await reviewReadable(service);
    const save = exportView.getByRole<HTMLButtonElement>("button", {
      name: copy.export.save,
    });
    await user.dblClick(save);
    expect(service.exportHistory).toHaveBeenCalledTimes(1);
    expect(save.disabled).toBe(true);
    expect(exportView.getByText(copy.export.saving)).toBeTruthy();
    expect(
      exportView.getByRole<HTMLButtonElement>("button", { name: copy.back })
        .disabled,
    ).toBe(true);
    complete({ status: "rejected", reason: "write_failed" });
    expect((await exportView.findByRole("alert")).textContent).toBe(
      copy.reasons.write_failed,
    );
    service.exportHistory.mockRejectedValueOnce(new Error("unreachable"));
    await user.click(save);
    expect((await exportView.findByRole("alert")).textContent).toBe(
      copy.export.failed,
    );
  });

  it("imports read-only by default and renders all reported fidelity categories", async () => {
    const service = fake();
    const { user, importView } = await setup(service);
    await user.click(
      importView.getByRole("button", { name: copy.import.chooseFile }),
    );
    const readOnly = await importView.findByRole<HTMLInputElement>("radio", {
      name: copy.mode.read_only,
      description: copy.import.modeHint.read_only,
    });
    expect(readOnly.checked).toBe(true);
    expect(
      importView.getByRole<HTMLInputElement>("radio", {
        name: copy.mode.same_account,
        description: copy.reasons.wrong_account,
      }).disabled,
    ).toBe(true);
    expect(service.importHistory).not.toHaveBeenCalled();
    expect(
      await axeViolations(
        screen.getByRole("region", { name: copy.import.title }),
      ),
    ).toEqual([]);
    await user.click(
      importView.getByRole("button", { name: copy.import.start }),
    );
    const result = await importView.findByRole("heading", {
      name: copy.import.imported.read_only,
    });
    expect(document.activeElement).toBe(result);
    const intent: ImportIntent = { handle: source.handle, mode: "read_only" };
    expect(service.importHistory.mock.calls).toEqual([[intent]]);
    const report = within(
      importView.getByRole("region", { name: copy.report.title }),
    );
    for (const title of Object.values(copy.report.section))
      expect(report.getByText(title)).toBeTruthy();
    expect(report.getByText("History records kept (3)")).toBeTruthy();
    expect(
      await axeViolations(
        screen.getByRole("region", { name: copy.import.title }),
      ),
    ).toEqual([]);
  });

  it("takes the selected import mode from the user and takes the result mode from Core", async () => {
    const service = fake({
      chooseImportSource: () =>
        Promise.resolve({
          status: "selected",
          source: {
            ...source,
            modes: { read_only: "available", same_account: "available" },
          },
        }),
    });
    service.importHistory.mockResolvedValueOnce({
      status: "rejected",
      reason: "unauthorized_device",
    });
    const { user, importView } = await setup(service);
    await user.click(
      importView.getByRole("button", { name: copy.import.chooseFile }),
    );
    const restore = await importView.findByRole<HTMLInputElement>("radio", {
      name: copy.mode.same_account,
    });
    expect(restore.checked).toBe(false);
    await user.click(restore);
    await user.click(
      importView.getByRole("button", { name: copy.import.start }),
    );
    expect((await importView.findByRole("alert")).textContent).toBe(
      copy.reasons.unauthorized_device,
    );
    expect(restore.checked).toBe(true);
    expect(service.importHistory.mock.calls).toEqual([
      [{ handle: source.handle, mode: "same_account" }],
    ]);
    await user.click(
      importView.getByRole("button", { name: copy.import.start }),
    );
    await importView.findByRole("heading", {
      name: copy.import.imported.read_only,
    });
    expect(
      importView.queryByRole("heading", {
        name: copy.import.imported.same_account,
      }),
    ).toBeNull();
  });

  it("shows a refused file and keeps import disabled until a file is selected", async () => {
    const service = fake();
    service.chooseImportSource.mockResolvedValueOnce({
      status: "rejected",
      reason: "encrypted_unsupported",
    });
    service.chooseImportSource.mockResolvedValueOnce({ status: "cancelled" });
    const { user, importView } = await setup(service);
    const choose = importView.getByRole("button", {
      name: copy.import.chooseFile,
    });
    await user.click(choose);
    expect((await importView.findByRole("alert")).textContent).toBe(
      copy.reasons.encrypted_unsupported,
    );
    expect(
      importView.getByRole<HTMLButtonElement>("button", {
        name: copy.import.start,
      }).disabled,
    ).toBe(true);
    await user.click(choose);
    await importView.findByText(copy.import.fileCancelled);
    expect(importView.queryByRole("alert")).toBeNull();
    await user.click(choose);
    expect(
      (
        await importView.findByRole<HTMLButtonElement>("button", {
          name: copy.import.start,
        })
      ).disabled,
    ).toBe(false);
  });

  it("announces a slow import and preserves the selected archive when import is refused", async () => {
    let complete: (answer: ImportAnswer) => void = () => {
      throw new Error("no import pending");
    };
    const service = fake({
      importHistory: () =>
        new Promise((resolve) => {
          complete = resolve;
        }),
    });
    const { user, importView } = await setup(service);
    await user.click(
      importView.getByRole("button", { name: copy.import.chooseFile }),
    );
    const start = await importView.findByRole<HTMLButtonElement>("button", {
      name: copy.import.start,
    });
    await user.dblClick(start);
    expect(service.importHistory).toHaveBeenCalledTimes(1);
    expect(start.disabled).toBe(true);
    expect(importView.getByText(copy.import.importing)).toBeTruthy();
    expect(
      importView.getByRole<HTMLButtonElement>("button", {
        name: copy.import.changeFile,
      }).disabled,
    ).toBe(true);
    complete({ status: "rejected", reason: "integrity_failure" });
    expect((await importView.findByRole("alert")).textContent).toBe(
      copy.reasons.integrity_failure,
    );
    expect(importView.getByText(source.name)).toBeTruthy();
    expect(start.disabled).toBe(false);
  });

  it("drops the visible file and mode when the host changes profiles", async () => {
    const { rerender } = render(<Portability service={fake()} copy={copy} />);
    await userEvent.click(
      await screen.findByRole("button", { name: copy.import.chooseFile }),
    );
    await screen.findByText(source.name);
    rerender(<Portability service={fake()} copy={copy} />);
    await screen.findByRole("button", { name: copy.import.chooseFile });
    expect(screen.queryByText(source.name)).toBeNull();
  });

  it("renders an empty report without inferring that everything transferred", () => {
    render(
      <PortabilityReportView
        report={{ ...archiveReport, details: {} }}
        copy={copy}
      />,
    );
    expect(screen.getByText(copy.report.empty)).toBeTruthy();
  });
});
