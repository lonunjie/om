import type { ReactElement } from "react";
import "../conversations/conversations.css";

/** A 32-bit mix of a string (FNV-1a with a final avalanche). Not a security primitive: it only picks a picture. */
const mix = (s: string): number => {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) { h ^= s.charCodeAt(i); h = Math.imul(h, 0x01000193); }
  h ^= h >>> 16; h = Math.imul(h, 0x85ebca6b); h ^= h >>> 13; h = Math.imul(h, 0xc2b2ae35); h ^= h >>> 16;
  return h >>> 0;
};

/** Which of the 15 left-half cells of a 5 by 5 mirrored grid are filled for a seed (AVATAR_AND_FALLBACK_IDENTITY_MEDIA: abstract, stable, no human traits). */
export const patternCells = (seed: string): boolean[] => {
  const bits = mix(`om-avatar-v0:${seed}`);
  return Array.from({ length: 15 }, (_v, i) => ((bits >>> i) & 1) === 1);
};

/**
 * A stable abstract pattern for a person who shares a name with someone else. It is decoration only: hidden from assistive technology,
 * because the "1 of 2" text beside the name carries the same information. The seed is an opaque id the screen already has.
 */
export const PatternAvatar = ({ seed, size = "md" }: { readonly seed: string; readonly size?: "md" | "lg" }) => {
  const cells = patternCells(seed);
  const rects: ReactElement[] = [];
  for (let row = 0; row < 5; row++) {
    for (let col = 0; col < 5; col++) {
      const left = col < 3 ? col : 4 - col;
      if (cells[row * 3 + left]) rects.push(<rect key={`${String(row)}-${String(col)}`} x={col} y={row} width="1" height="1" />);
    }
  }
  return (
    <svg className="om-avatar om-avatar--pattern" data-size={size} aria-hidden="true" focusable="false" viewBox="-0.5 -0.5 6 6">{rects}</svg>
  );
};
