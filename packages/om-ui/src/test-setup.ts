// Shared Vitest setup: Testing Library's 1 s async default is too tight when many jsdom suites (real WebCrypto vault
// ceremonies, axe) run in parallel on a busy machine. Longer waits only slow a failing test, never a passing one.
import { configure } from "@testing-library/react";

configure({ asyncUtilTimeout: 10000 });
