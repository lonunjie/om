import type { PointerEvent } from "react";

/** Where a press began, as CSS variables on the control: the press circle grows from that point (centre for a key press). */
export const pressOrigin = (e: PointerEvent<HTMLElement>): void => {
  const box = e.currentTarget.getBoundingClientRect();
  e.currentTarget.style.setProperty("--om-press-x", `${String(Math.round(e.clientX - box.left))}px`);
  e.currentTarget.style.setProperty("--om-press-y", `${String(Math.round(e.clientY - box.top))}px`);
};
