import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

afterEach(() => {
  cleanup();
});

// jsdom lacks ResizeObserver, which TanStack Virtual needs to measure the
// scroll viewport. The stub reports an empty rect so virtualized windows stay
// at their initial bounded size in tests.
if (typeof globalThis.ResizeObserver === "undefined") {
  globalThis.ResizeObserver = class {
    callback: ResizeObserverCallback;
    constructor(callback: ResizeObserverCallback) {
      this.callback = callback;
    }
    observe(): void {
      this.callback([], this as unknown as ResizeObserver);
    }
    unobserve(): void {}
    disconnect(): void {}
  };
}
