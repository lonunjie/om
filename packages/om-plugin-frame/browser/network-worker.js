// SPDX-License-Identifier: MPL-2.0
// The measured spike's probe code is served verbatim by the local test server.
importScripts("/probe.js");

const violations = observeCsp();
self.onmessage = async ({ data: urls }) => {
  const results = await Promise.all(Object.entries(urls).map(([api, url]) => networkProbe(api, url)));
  await new Promise((resolve) => setTimeout(resolve, 250));
  self.postMessage({ results, violations, crossOriginIsolated });
};
