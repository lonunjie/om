import type { ReactNode } from "react";
import { IconButton } from "../shell/primitives/IconButton";
import { Icon } from "../shell/primitives/icons";
import type { OnboardingCopy } from "./copy";

export interface OnboardingFrameProps {
  readonly copy: OnboardingCopy["frame"];
  /** 1-based position among the steps the person will see; omit on a step that is not part of the count. */
  readonly progress?: { readonly n: number; readonly total: number };
  /** Changes with the step: the card re-enters (a short fade) so the person sees that something new arrived. */
  readonly stepKey: string;
  /** A way out when the page was opened from inside the app (adding an identity): a round arrow beside the brand. Omit on true first run. */
  readonly back?: { readonly label: string; readonly onBack: () => void };
  readonly children: ReactNode;
}

/**
 * First-run page chrome shared by every step: brand, where you are, then one card. Same layout on a phone and a desktop
 * window (UI_UX_SYSTEM "one interaction language"): a single centred column, safe-area padding, no horizontal scroll.
 */
export function OnboardingFrame({ copy, progress, stepKey, back, children }: OnboardingFrameProps) {
  return (
    <main className="om-onb">
      <header className="om-onb__top">
        <span className="om-onb__top-lead">
          {back ? <IconButton className="om-onb__back" label={back.label} icon={<Icon name="back" />} onClick={back.onBack} /> : null}
          <span className="om-onb__brand" translate="no">{copy.brand}<span className="om-onb__beta">{copy.beta}</span></span>
        </span>
        {progress ? (
          <div className="om-onb__progress" role="group" aria-label={copy.step(progress.n, progress.total)}>
            <span className="om-onb__step-label">{copy.step(progress.n, progress.total)}</span>
          </div>
        ) : null}
      </header>
      <div className="om-onb__card" key={stepKey}>{children}</div>
    </main>
  );
}
