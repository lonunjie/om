const hex = (b: ArrayBuffer): string => Array.from(new Uint8Array(b), (x) => x.toString(16).padStart(2, "0")).join("");

export const sha256Hex = async (data: Uint8Array | ArrayBuffer): Promise<string> =>
  hex(await crypto.subtle.digest("SHA-256", data as BufferSource));
