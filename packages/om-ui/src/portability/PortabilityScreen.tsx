// SPDX-License-Identifier: MPL-2.0

import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import { SecurityNoticeBanner } from "../shell/theme/SecurityNoticeBanner";
import { PortabilityReportView } from "./PortabilityReportView";
import type { PortabilityCopy } from "./copy";
import {
  exportContent,
  type ExportContent,
  type ExportFormat,
  type ExportIntent,
  type HistoryChoice,
  type ImportMode,
  type ImportSource,
  type PortabilityService,
  type PortabilityView,
} from "./portability";
import "./portability.css";

const formats = ["archive", "readable"] as const;
const modes = ["read_only", "same_account"] as const;
const historyKinds = ["chat", "space", "publication"] as const;

type RequestState<T> =
  | { readonly phase: "idle" | "pending" | "failed" }
  | { readonly phase: "answered"; readonly answer: T };

function useRequest<T>() {
  const [state, setState] = useState<RequestState<T>>({ phase: "idle" });
  const inFlight = useRef(false);
  async function run(request: () => Promise<T>) {
    if (inFlight.current) return;
    inFlight.current = true;
    setState({ phase: "pending" });
    try {
      setState({ phase: "answered", answer: await request() });
    } catch {
      setState({ phase: "failed" });
    } finally {
      inFlight.current = false;
    }
  }
  return {
    answer: state.phase === "answered" ? state.answer : null,
    busy: state.phase === "pending",
    failed: state.phase === "failed",
    reset: () => {
      setState({ phase: "idle" });
    },
    run,
  };
}

function Choice({
  type,
  name,
  checked,
  disabled,
  label,
  description,
  onChange,
}: {
  readonly type: "radio" | "checkbox";
  readonly name?: string;
  readonly checked: boolean;
  readonly disabled?: boolean;
  readonly label: ReactNode;
  readonly description?: string | undefined;
  readonly onChange: (checked: boolean) => void;
}) {
  const descriptionId = useId();
  return (
    <div>
      <label className="om-portability__choice">
        <input
          type={type}
          name={name}
          checked={checked}
          disabled={disabled}
          aria-describedby={
            description === undefined ? undefined : descriptionId
          }
          onChange={(event) => {
            onChange(event.target.checked);
          }}
        />
        {label}
      </label>
      {description === undefined ? null : (
        <Text id={descriptionId} size="sm" tone="secondary">
          {description}
        </Text>
      )}
    </div>
  );
}

function Status({
  id,
  status,
  error,
}: {
  readonly id?: string;
  readonly status: string | null;
  readonly error: string | null;
}) {
  return (
    <>
      <Text id={id} role="status">
        {status}
      </Text>
      {error === null ? null : <Text role="alert">{error}</Text>}
    </>
  );
}

type ServiceProps = {
  readonly service: PortabilityService;
  readonly copy: PortabilityCopy;
};

function ExportWizard({
  service,
  copy,
  view,
}: ServiceProps & { readonly view: PortabilityView }) {
  const t = copy.export;
  const [format, setFormat] = useState<ExportFormat | null>(null);
  const [all, setAll] = useState(false);
  const [selected, setSelected] = useState<readonly HistoryChoice[]>([]);
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [contents, setContents] = useState<readonly ExportContent[]>([]);
  const [review, setReview] = useState(false);
  const [includeDisappearing, setIncludeDisappearing] = useState(false);
  const request =
    useRequest<Awaited<ReturnType<PortabilityService["exportHistory"]>>>();
  const titleId = useId();
  const formatName = useId();
  const historyName = useId();
  const scopeHintId = useId();
  const dateHintId = useId();
  const plaintextId = useId();
  const retentionId = useId();
  const disappearingId = useId();
  const statusId = useId();
  const heading = useRef<HTMLHeadingElement>(null);
  const saved = request.answer?.status === "saved" ? request.answer : null;
  const step = saved ? "saved" : review ? "review" : "scope";
  const previousStep = useRef(step);
  useEffect(() => {
    if (previousStep.current !== step) heading.current?.focus();
    previousStep.current = step;
  }, [step]);

  const choices = format === null ? null : view.formats[format];
  const enabledChoices = typeof choices === "object" ? choices : null;
  const allHistory = all && Boolean(enabledChoices?.history.length);
  const availableContents = contents.filter(
    (item) => enabledChoices?.contents[item] === "available",
  );
  const chosenHistory =
    enabledChoices?.history.filter((target) =>
      selected.some(({ id, kind }) => target.id === id && target.kind === kind),
    ) ?? [];
  const anyFormatAvailable = formats.some(
    (option) => typeof view.formats[option] !== "string",
  );
  const reason =
    enabledChoices === null
      ? anyFormatAvailable
        ? t.chooseFormat
        : t.noFormats
      : allHistory || chosenHistory.length || availableContents.length
        ? null
        : t.chooseScope;

  function save() {
    if (format === null || reason !== null || request.busy || saved !== null)
      return;
    const scope = {
      history: allHistory
        ? ("all" as const)
        : chosenHistory.map(({ id, kind }) => ({ id, kind })),
      from: from || null,
      to: to || null,
      contents: availableContents,
    };
    const intent: ExportIntent =
      format === "archive"
        ? { format, scope }
        : { format, scope, includeDisappearing };
    void request.run(() => service.exportHistory(intent));
  }

  function back() {
    request.reset();
    setReview(false);
    setIncludeDisappearing(false);
  }

  function startAnother() {
    back();
    setFormat(null);
    setAll(false);
    setSelected([]);
    setFrom("");
    setTo("");
    setContents([]);
  }

  function toggleHistory(target: HistoryChoice, checked: boolean) {
    setSelected((current) =>
      checked
        ? [...current, target]
        : current.filter(
            ({ id, kind }) => target.id !== id || target.kind !== kind,
          ),
    );
  }

  const status = request.busy
    ? t.saving
    : request.answer?.status === "cancelled"
      ? t.cancelled
      : null;
  const error = request.failed
    ? t.failed
    : request.answer?.status === "rejected"
      ? copy.reasons[request.answer.reason]
      : null;

  return (
    <section className="om-portability__section" aria-labelledby={titleId}>
      <Text as="h2" id={titleId} size="lg" weight="bold">
        {t.title}
      </Text>
      <h3 ref={heading} className="om-text" data-weight="bold" tabIndex={-1}>
        {step === "saved" ? t.saved : step === "review" ? t.review : t.scope}
      </h3>
      {saved ? (
        <>
          <PortabilityReportView report={saved.report} copy={copy} />
          <div className="om-portability__actions">
            <Button onClick={startAnother}>{t.again}</Button>
          </div>
        </>
      ) : review && format !== null ? (
        <>
          <div className="om-portability__stack">
            <Text weight="bold">{copy.format[format]}</Text>
            <ul className="om-portability__list">
              {allHistory ? (
                <li>{t.allHistory}</li>
              ) : (
                chosenHistory.map((item) => (
                  <li key={JSON.stringify([item.kind, item.id])}>
                    <bdi>{item.name}</bdi>
                  </li>
                ))
              )}
              {from ? (
                <li>
                  {t.from} <bdi>{from}</bdi>
                </li>
              ) : null}
              {to ? (
                <li>
                  {t.to} <bdi>{to}</bdi>
                </li>
              ) : null}
              {availableContents.map((item) => (
                <li key={item}>{t.content[item]}</li>
              ))}
            </ul>
          </div>
          <div className="om-portability__stack" id={plaintextId}>
            <SecurityNoticeBanner
              reasonCode="unencrypted_export"
              label={t.plaintext}
            />
          </div>
          <Text id={retentionId}>
            {format === "archive" ? t.archiveRetention : t.readableRetention}
          </Text>
          {format === "readable" ? (
            <div id={disappearingId}>
              <Choice
                type="checkbox"
                checked={includeDisappearing}
                disabled={request.busy}
                label={t.includeDisappearing}
                description={t.disappearingWarning}
                onChange={setIncludeDisappearing}
              />
            </div>
          ) : null}
          <div className="om-portability__actions">
            <Button
              disabled={request.busy}
              aria-describedby={request.busy ? statusId : undefined}
              onClick={back}
            >
              {copy.back}
            </Button>
            <Button
              variant="primary"
              disabled={request.busy}
              aria-describedby={[
                plaintextId,
                retentionId,
                ...(includeDisappearing ? [disappearingId] : []),
                ...(request.busy ? [statusId] : []),
              ].join(" ")}
              onClick={save}
            >
              {t.save}
            </Button>
          </div>
          <Status id={statusId} status={status} error={error} />
        </>
      ) : (
        <>
          <fieldset className="om-portability__fieldset">
            <legend>{t.format}</legend>
            {formats.map((option) => {
              const availability = view.formats[option];
              const unavailable = typeof availability === "string";
              return (
                <Choice
                  key={option}
                  type="radio"
                  name={formatName}
                  checked={option === format}
                  disabled={unavailable}
                  label={copy.format[option]}
                  description={
                    unavailable
                      ? copy.reasons[availability]
                      : t.formatHint[option]
                  }
                  onChange={() => {
                    setFormat(option);
                  }}
                />
              );
            })}
          </fieldset>
          {enabledChoices === null ? null : (
            <>
              <fieldset className="om-portability__fieldset">
                <legend>{t.history}</legend>
                <Choice
                  type="radio"
                  name={historyName}
                  checked={!allHistory}
                  label={t.chooseHistory}
                  onChange={() => {
                    setAll(false);
                  }}
                />
                <Choice
                  type="radio"
                  name={historyName}
                  checked={allHistory}
                  disabled={!enabledChoices.history.length}
                  label={t.allHistory}
                  description={
                    enabledChoices.history.length ? undefined : t.noHistory
                  }
                  onChange={() => {
                    setAll(true);
                  }}
                />
                {allHistory
                  ? null
                  : historyKinds.map((kind) => {
                      const targets = enabledChoices.history.filter(
                        (target) => target.kind === kind,
                      );
                      if (!targets.length) return null;
                      return (
                        <fieldset
                          key={kind}
                          className="om-portability__fieldset"
                        >
                          <legend>{t.historyKind[kind]}</legend>
                          {targets.map((target) => (
                            <Choice
                              key={target.id}
                              type="checkbox"
                              checked={chosenHistory.includes(target)}
                              label={<bdi>{target.name}</bdi>}
                              onChange={(checked) => {
                                toggleHistory(target, checked);
                              }}
                            />
                          ))}
                        </fieldset>
                      );
                    })}
              </fieldset>
              <fieldset
                className="om-portability__fieldset om-portability__stack"
                aria-describedby={dateHintId}
              >
                <legend>{t.dates}</legend>
                <Text id={dateHintId} size="sm" tone="secondary">
                  {t.dateHint}
                </Text>
                <label className="om-portability__date">
                  {t.from}
                  <input
                    type="date"
                    value={from}
                    onChange={(event) => {
                      setFrom(event.target.value);
                    }}
                  />
                </label>
                <label className="om-portability__date">
                  {t.to}
                  <input
                    type="date"
                    value={to}
                    onChange={(event) => {
                      setTo(event.target.value);
                    }}
                  />
                </label>
              </fieldset>
              <fieldset className="om-portability__fieldset">
                <legend>{t.contents}</legend>
                {exportContent.map((item) => {
                  const availability = enabledChoices.contents[item];
                  return (
                    <Choice
                      key={item}
                      type="checkbox"
                      checked={availableContents.includes(item)}
                      disabled={availability !== "available"}
                      label={t.content[item]}
                      description={
                        availability === "available"
                          ? undefined
                          : copy.reasons[availability]
                      }
                      onChange={(checked) => {
                        setContents((current) =>
                          checked
                            ? [...current, item]
                            : current.filter((value) => value !== item),
                        );
                      }}
                    />
                  );
                })}
              </fieldset>
            </>
          )}
          <div className="om-portability__actions">
            <Button
              variant="primary"
              disabled={reason !== null}
              aria-describedby={reason === null ? undefined : scopeHintId}
              onClick={() => {
                setReview(true);
              }}
            >
              {t.review}
            </Button>
          </div>
          {reason === null ? null : (
            <Text id={scopeHintId} size="sm" tone="secondary">
              {reason}
            </Text>
          )}
        </>
      )}
    </section>
  );
}

function ImportHistory({
  service,
  copy,
  view,
}: ServiceProps & { readonly view: PortabilityView }) {
  const t = copy.import;
  const [source, setSource] = useState<ImportSource | null>(null);
  const [mode, setMode] = useState<ImportMode>("read_only");
  const picker =
    useRequest<Awaited<ReturnType<PortabilityService["chooseImportSource"]>>>();
  const request =
    useRequest<Awaited<ReturnType<PortabilityService["importHistory"]>>>();
  const titleId = useId();
  const modeName = useId();
  const chooseHintId = useId();
  const importHintId = useId();
  const statusId = useId();
  const resultHeading = useRef<HTMLHeadingElement>(null);
  const imported =
    request.answer?.status === "imported" ? request.answer : null;
  useEffect(() => {
    if (imported !== null) resultHeading.current?.focus();
  }, [imported]);

  const unavailable =
    view.import === "available" ? null : copy.reasons[view.import];
  const busy = request.busy || picker.busy;
  const selectedMode = source?.modes[mode];
  const reason =
    unavailable ??
    (selectedMode === undefined
      ? t.chooseFirst
      : selectedMode === "available"
        ? null
        : copy.reasons[selectedMode]);

  function choose() {
    if (busy || unavailable !== null) return;
    void picker.run(async () => {
      const answer = await service.chooseImportSource();
      if (answer.status === "selected") {
        setSource(answer.source);
        setMode("read_only");
        request.reset();
      }
      return answer;
    });
  }

  function start() {
    if (source === null || busy || reason !== null || imported !== null) return;
    picker.reset();
    void request.run(() =>
      service.importHistory({ handle: source.handle, mode }),
    );
  }

  const status = picker.busy
    ? t.choosing
    : request.busy
      ? t.importing
      : picker.answer?.status === "cancelled"
        ? t.fileCancelled
        : request.answer?.status === "cancelled"
          ? t.cancelled
          : null;
  const error = busy
    ? null
    : picker.failed
      ? t.chooseFailed
      : picker.answer?.status === "rejected"
        ? copy.reasons[picker.answer.reason]
        : request.failed
          ? t.failed
          : request.answer?.status === "rejected"
            ? copy.reasons[request.answer.reason]
            : null;

  return (
    <section className="om-portability__section" aria-labelledby={titleId}>
      <Text as="h2" id={titleId} size="lg" weight="bold">
        {t.title}
      </Text>
      <div className="om-portability__actions">
        <Button
          disabled={busy || unavailable !== null}
          aria-describedby={
            unavailable !== null ? chooseHintId : busy ? statusId : undefined
          }
          onClick={choose}
        >
          {source === null ? t.chooseFile : t.changeFile}
        </Button>
      </div>
      {unavailable === null ? null : (
        <Text id={chooseHintId} tone="secondary">
          {unavailable}
        </Text>
      )}
      {source === null ? null : (
        <Text aria-live="polite">
          <bdi>{source.name}</bdi>
        </Text>
      )}
      {imported === null ? (
        <>
          {source === null ? (
            <Text tone="secondary">{t.modeHint.read_only}</Text>
          ) : (
            <fieldset className="om-portability__fieldset" disabled={busy}>
              <legend>{t.mode}</legend>
              {modes.map((option) => {
                const availability = source.modes[option];
                return (
                  <Choice
                    key={option}
                    type="radio"
                    name={modeName}
                    checked={option === mode}
                    label={copy.mode[option]}
                    disabled={availability !== "available"}
                    description={
                      availability === "available"
                        ? t.modeHint[option]
                        : copy.reasons[availability]
                    }
                    onChange={() => {
                      setMode(option);
                      request.reset();
                    }}
                  />
                );
              })}
            </fieldset>
          )}
          <div className="om-portability__actions">
            <Button
              variant="primary"
              disabled={busy || reason !== null}
              aria-describedby={
                busy
                  ? statusId
                  : unavailable !== null
                    ? chooseHintId
                    : reason === null
                      ? undefined
                      : importHintId
              }
              onClick={start}
            >
              {t.start}
            </Button>
          </div>
          {reason === null || unavailable !== null ? null : (
            <Text id={importHintId} size="sm" tone="secondary">
              {reason}
            </Text>
          )}
        </>
      ) : (
        <>
          <h3
            ref={resultHeading}
            className="om-text"
            data-weight="bold"
            tabIndex={-1}
          >
            {t.imported[imported.mode]}
          </h3>
          <PortabilityReportView report={imported.report} copy={copy} />
        </>
      )}
      <Status id={statusId} status={status} error={error} />
    </section>
  );
}

type Shown = {
  readonly service: PortabilityService;
  readonly view: PortabilityView | null;
};

export function Portability({
  service,
  copy,
}: {
  readonly service?: PortabilityService;
  readonly copy: PortabilityCopy;
}) {
  const [shown, setShown] = useState<Shown | null>(null);
  const [attempt, setAttempt] = useState(0);
  const titleId = useId();
  useEffect(() => {
    if (service === undefined) return;
    let live = true;
    service.state().then(
      (view) => {
        if (live) setShown({ service, view });
      },
      () => {
        if (live) setShown({ service, view: null });
      },
    );
    return () => {
      live = false;
    };
  }, [service, attempt]);

  if (service === undefined) return null;
  const current = shown?.service === service ? shown : null;
  return (
    <section className="om-portability" aria-labelledby={titleId}>
      <Text as="h1" id={titleId} size="xl" weight="bold">
        {copy.title}
      </Text>
      {current === null ? (
        <Text role="status">{copy.loading}</Text>
      ) : current.view === null ? (
        <>
          <Text role="alert">{copy.loadFailed}</Text>
          <div className="om-portability__actions">
            <Button
              onClick={() => {
                setShown(null);
                setAttempt((count) => count + 1);
              }}
            >
              {copy.retry}
            </Button>
          </div>
        </>
      ) : (
        <>
          <ExportWizard service={service} view={current.view} copy={copy} />
          <ImportHistory service={service} view={current.view} copy={copy} />
        </>
      )}
    </section>
  );
}
