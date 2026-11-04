import { useEffect, useRef, type RefObject } from "react";

/**
 * Back that follows the finger (MOTION_AND_PERFORMANCE.md, "Back gesture"): on a touch screen, a drag that starts at the leading
 * edge of the open pane moves the pane with the finger. Letting go past a third of the width (or with a quick flick) goes back;
 * letting go earlier, or cancelling, puts the pane back with nothing lost. It is only an accelerator: every screen with this
 * gesture also has a Back control and the browser's own Back. The motion is a transform set directly on the element, so it
 * tracks the finger without waiting for a render; the settle after release uses the motion tokens (instant under reduced motion).
 */
const EDGE_PX = 28;
const COMMIT_FRACTION = 0.35;
const FLICK_PX_PER_MS = 0.6;

interface Drag { readonly id: number; readonly x0: number; readonly y0: number; readonly t0: number; engaged: boolean; dx: number; readonly sign: 1 | -1; readonly width: number }

export function useEdgeSwipeBack(pane: RefObject<HTMLElement | null>, enabled: boolean, onBack: (() => void) | undefined): void {
  // A fresh callback on every render must not restart a drag that is under way.
  const back = useRef(onBack);
  back.current = onBack;
  const active = enabled && onBack !== undefined;
  useEffect(() => {
    const el = pane.current;
    if (el === null || !active) return;
    let drag: Drag | null = null;

    const reset = (animate: boolean): void => {
      el.removeAttribute("data-dragging");
      if (animate) el.setAttribute("data-settling", "true");
      el.style.transform = "";
      if (animate) {
        const clear = (): void => { el.removeAttribute("data-settling"); el.removeEventListener("transitionend", clear); };
        el.addEventListener("transitionend", clear);
        // Reduced motion (or no transition at all) never fires transitionend.
        setTimeout(clear, 400);
      }
    };

    const onDown = (e: PointerEvent): void => {
      if (e.pointerType !== "touch" || !e.isPrimary) return;
      const box = el.getBoundingClientRect();
      const rtl = getComputedStyle(el).direction === "rtl";
      const fromEdge = rtl ? box.right - e.clientX : e.clientX - box.left;
      if (fromEdge < 0 || fromEdge > EDGE_PX) return;
      drag = { id: e.pointerId, x0: e.clientX, y0: e.clientY, t0: e.timeStamp, engaged: false, dx: 0, sign: rtl ? -1 : 1, width: box.width };
    };

    const onMove = (e: PointerEvent): void => {
      if (drag === null || e.pointerId !== drag.id) return;
      const dx = (e.clientX - drag.x0) * drag.sign;
      const dy = e.clientY - drag.y0;
      if (!drag.engaged) {
        if (Math.abs(dy) > 12 && Math.abs(dy) > Math.abs(dx)) { drag = null; return; } // a vertical scroll, not Back
        if (dx < 8) return;
        drag.engaged = true;
        el.setAttribute("data-dragging", "true");
        try { el.setPointerCapture(e.pointerId); } catch { /* the pointer is already gone */ }
      }
      drag.dx = Math.max(0, dx);
      el.style.transform = `translateX(${String(drag.dx * drag.sign)}px)`;
    };

    const finish = (e: PointerEvent, cancelled: boolean): void => {
      if (drag === null || e.pointerId !== drag.id) return;
      const d = drag;
      drag = null;
      if (!d.engaged) return;
      const fast = d.dx / Math.max(1, e.timeStamp - d.t0) > FLICK_PX_PER_MS;
      const commit = !cancelled && (d.dx > d.width * COMMIT_FRACTION || (fast && d.dx > 24));
      if (commit) { reset(false); back.current?.(); } else reset(true);
    };

    const onUp = (e: PointerEvent): void => { finish(e, false); };
    const onCancel = (e: PointerEvent): void => { finish(e, true); };
    el.addEventListener("pointerdown", onDown);
    el.addEventListener("pointermove", onMove);
    el.addEventListener("pointerup", onUp);
    el.addEventListener("pointercancel", onCancel);
    return () => {
      el.removeEventListener("pointerdown", onDown);
      el.removeEventListener("pointermove", onMove);
      el.removeEventListener("pointerup", onUp);
      el.removeEventListener("pointercancel", onCancel);
      reset(false);
    };
  }, [pane, active]);
}
