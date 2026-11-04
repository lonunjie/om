import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import type { OnboardingCopy } from "./copy";
import { StepHeading } from "./StepHeading";

export interface FirstRunChoiceProps {
  readonly copy: NonNullable<OnboardingCopy["join"]>;
  readonly onStartNew: () => void;
  readonly onUseExisting: () => void;
}

/**
 * The new device's first screen (UX brief section 5): two equal choices, "Start new" and "I already use OM". Shown only when the
 * host says Core can link a device, so nobody is offered a flow that ends in "unsupported". Neither is the dominant action.
 */
export function FirstRunChoice({ copy, onStartNew, onUseExisting }: FirstRunChoiceProps) {
  return (
    <section className="om-onb__step" aria-labelledby="om-onb-choice-h">
      <StepHeading id="om-onb-choice-h">{copy.choiceHeading}</StepHeading>
      <div className="om-onb__choices">
        <Button onClick={onStartNew}>{copy.startNew}</Button>
        <Text as="p" size="sm" tone="secondary">{copy.startNewHint}</Text>
        <Button onClick={onUseExisting}>{copy.useExisting}</Button>
        <Text as="p" size="sm" tone="secondary">{copy.useExistingHint}</Text>
      </div>
    </section>
  );
}
