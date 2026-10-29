import { describeLimits } from "@om/platform-api";
import type { VaultWrappingInfo } from "@om/platform-api";
import { Text } from "../shell/primitives/Text";
import type { OnboardingCopy } from "./copy";
import type { OnboardingPlatform } from "./platform";

export interface ProtectionSummaryProps {
  readonly wrapping: VaultWrappingInfo;
  readonly platform: OnboardingPlatform;
  readonly copy: OnboardingCopy["protect"];
}

/**
 * What protects the chats, shown after setup without blocking it. Weaker-than-expected protection is always visible as a
 * note; the longer explanation sits behind a disclosure so an ordinary user is not asked to read or tap through it.
 */
export function ProtectionSummary({ wrapping, platform, copy }: ProtectionSummaryProps) {
  return (
    <div className="om-onb__result">
      {wrapping.reducedAtRestProperty ? <p role="note" className="om-onb__notice" data-code="reduced_at_rest">{copy.reducedAtRest}</p> : null}
      {wrapping.method === "device-key" ? (
        <p role="note" className="om-onb__notice" data-code="device_key_limit">{copy.deviceKeyLimit} {copy.nativeRecommended}</p>
      ) : null}
      <details className="om-onb__more">
        <summary>{copy.resultHeading}</summary>
        <Text as="p">{copy.method[wrapping.method]}</Text>
        <Text as="p" tone="secondary">{copy.reason[wrapping.reason]}</Text>
        {wrapping.method === "device-key" ? (
          <>
            <Text as="h4" size="sm" weight="bold">{copy.limitsHeading}</Text>
            <ul>{describeLimits(platform.capabilities()).map((c) => <li key={c}>{copy.limits[c]}</li>)}</ul>
          </>
        ) : null}
      </details>
    </div>
  );
}
