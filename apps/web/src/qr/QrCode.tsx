// Renders a QR code as React-built SVG (never an HTML string). Dark-on-light with a quiet zone so it scans in either theme.
import { qrModules } from "@om/web-platform";

const QUIET = 4;

export function QrCode({ payload }: { readonly payload: string }) {
  const modules = qrModules(payload);
  if (modules === null) return null;
  const size = modules.length + QUIET * 2;
  const d: string[] = [];
  modules.forEach((row, y) => {
    let x = 0;
    while (x < row.length) {
      if (!row[x]) { x++; continue; }
      const start = x;
      while (x < row.length && row[x]) x++;
      d.push(`M${String(start + QUIET)} ${String(y + QUIET)}h${String(x - start)}v1h-${String(x - start)}z`);
    }
  });
  return (
    <svg className="om-qr" viewBox={`0 0 ${String(size)} ${String(size)}`} role="img" aria-hidden="true" focusable="false" shapeRendering="crispEdges">
      <rect width={size} height={size} fill="#fff" />
      <path d={d.join("")} fill="#000" />
    </svg>
  );
}
