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

if (typeof globalThis.DOMMatrixReadOnly === "undefined") {
  // DOMMatrix's standard API has many parameters that a layout-neutral jsdom
  // identity shim intentionally ignores.
  /* eslint-disable @typescript-eslint/no-unused-vars */
  globalThis.DOMMatrixReadOnly = class {
    a = 1;
    b = 0;
    c = 0;
    d = 1;
    e = 0;
    f = 0;
    m11 = 1;
    m12 = 0;
    m13 = 0;
    m14 = 0;
    m21 = 0;
    m22 = 1;
    m23 = 0;
    m24 = 0;
    m31 = 0;
    m32 = 0;
    m33 = 1;
    m34 = 0;
    m41 = 0;
    m42 = 0;
    m43 = 0;
    m44 = 1;
    is2D = true;
    isIdentity = true;
    constructor(_transform?: string | number[]) {}
    flipX() {
      return this;
    }
    flipY() {
      return this;
    }
    inverse() {
      return this;
    }
    multiply(_other?: DOMMatrixInit) {
      return this;
    }
    rotate(_rotX?: number, _rotY?: number, _rotZ?: number) {
      return this;
    }
    rotateAxisAngle(_x?: number, _y?: number, _z?: number, _angle?: number) {
      return this;
    }
    rotateFromVector(_x?: number, _y?: number) {
      return this;
    }
    scale(
      _scaleX?: number,
      _scaleY?: number,
      _scaleZ?: number,
      _originX?: number,
      _originY?: number,
      _originZ?: number,
    ) {
      return this;
    }
    scale3d(_scale?: number, _originX?: number, _originY?: number, _originZ?: number) {
      return this;
    }
    scaleNonUniform(_scaleX?: number, _scaleY?: number) {
      return this;
    }
    skewX(_sx?: number) {
      return this;
    }
    skewY(_sy?: number) {
      return this;
    }
    toFloat32Array() {
      return new Float32Array(16);
    }
    toFloat64Array() {
      return new Float64Array(16);
    }
    toJSON() {
      return {};
    }
    toString() {
      return "matrix(1, 0, 0, 1, 0, 0)";
    }
    transformPoint(point?: DOMPointInit) {
      return new DOMPoint(point?.x, point?.y, point?.z, point?.w);
    }
    translate(_tx?: number, _ty?: number, _tz?: number) {
      return this;
    }
  } as unknown as typeof DOMMatrixReadOnly;
  /* eslint-enable @typescript-eslint/no-unused-vars */
}
