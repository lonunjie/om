import type { ReactNode } from "react";
import type { OnboardingCopy } from "./copy";

export interface OnboardingFrameProps {
  readonly copy: OnboardingCopy["frame"];
  /** 1-based position among the steps the person will see; omit on a step that is not part of the count. */
  readonly progress?: { readonly n: number; readonly total: number };
  /** Changes with the step: the card re-enters (a short fade) so the person sees that something new arrived. */
  readonly stepKey: string;
  readonly children: ReactNode;
}

/**
 * First-run page chrome shared by every step: brand, where you are, then one card. Same layout on a phone and a desktop
 * window (UI_UX_SYSTEM "one interaction language"): a single centred column, safe-area padding, no horizontal scroll.
 */
export function OnboardingFrame({ copy, progress, stepKey, children }: OnboardingFrameProps) {
  return (
    <main className="om-onb">
      <header className="om-onb__top">
        <span className="om-onb__brand" translate="no">{copy.brand}<span className="om-onb__beta">{copy.beta}</span></span>
        {progress ? (
          <div className="om-onb__progress" role="group" aria-label={copy.step(progress.n, progress.total)}>
            <span className="om-onb__dots" aria-hidden="true">
              {Array.from({ length: progress.total }, (_, i) => <span key={i} className="om-onb__dot" data-state={i + 1 < progress.n ? "done" : i + 1 === progress.n ? "current" : "todo"} />)}
            </span>
            <span className="om-onb__step-label">{copy.step(progress.n, progress.total)}</span>
          </div>
        ) : null}
      </header>
      <div className="om-onb__card" key={stepKey}>{children}</div>
    </main>
  );
}
