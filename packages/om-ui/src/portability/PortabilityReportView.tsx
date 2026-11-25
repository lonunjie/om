// SPDX-License-Identifier: MPL-2.0

import { useId } from "react";
import { Text } from "../shell/primitives/Text";
import type { PortabilityCopy } from "./copy";
import { reportSections, type PortabilityReport } from "./portability";

export function PortabilityReportView({
  report,
  copy,
}: {
  readonly report: PortabilityReport;
  readonly copy: PortabilityCopy;
}) {
  const headingId = useId();
  const destination = report.destination;
  const destinationLabel =
    destination === "archive" || destination === "readable"
      ? copy.format[destination]
      : copy.mode[destination];

  return (
    <section className="om-portability__stack" aria-labelledby={headingId}>
      <Text as="h4" id={headingId} weight="bold">
        {copy.report.title}
      </Text>
      <dl className="om-portability__facts">
        <dt>{copy.report.source}</dt>
        <dd>{copy.report.from[report.source]}</dd>
        <dt>{copy.report.destination}</dt>
        <dd>{destinationLabel}</dd>
      </dl>
      {reportSections.map((section) => {
        const details = report.details[section];
        if (!details?.length) return null;
        return (
          <div key={section}>
            <Text weight="bold">{copy.report.section[section]}</Text>
            <ul className="om-portability__list">
              {details.map(({ code, count }) => (
                <li key={code}>
                  {count === undefined
                    ? copy.report.detail[code]
                    : copy.report.count(copy.report.detail[code], count)}
                </li>
              ))}
            </ul>
          </div>
        );
      })}
      {reportSections.every((section) => !report.details[section]?.length) ? (
        <Text tone="secondary">{copy.report.empty}</Text>
      ) : null}
    </section>
  );
}
