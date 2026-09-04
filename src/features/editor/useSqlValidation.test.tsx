import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { validateQuery, type SqlValidation } from "../../lib/commands";
import { useSqlValidation } from "./useSqlValidation";

vi.mock("../../lib/commands", () => ({ validateQuery: vi.fn() }));

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

const problem = (revision: number): SqlValidation => ({
  revision,
  diagnostics: [
    {
      code: "sql.catalog",
      message: 'Table with name "missing" does not exist',
      severity: "error",
      from: 14,
      to: 21,
    },
  ],
});

describe("useSqlValidation", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.clearAllMocks();
  });

  it("debounces edits and validates only the settled immutable revision", async () => {
    vi.mocked(validateQuery).mockResolvedValue({ revision: 2, diagnostics: [] });
    const { result, rerender } = renderHook(
      ({ sql }) => useSqlValidation("p1", "tab1", sql, null, 650),
      {
        initialProps: { sql: "SELECT * FR" },
      },
    );
    expect(result.current.status).toBe("editing");

    await act(async () => {
      await vi.advanceTimersByTimeAsync(400);
    });
    rerender({ sql: "SELECT * FROM orders" });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(649);
    });
    expect(validateQuery).not.toHaveBeenCalled();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    expect(validateQuery).toHaveBeenCalledTimes(1);
    expect(validateQuery).toHaveBeenCalledWith("p1", "SELECT * FROM orders", 2);
    expect(result.current.status).toBe("clean");
  });

  it("clears old diagnostics immediately and discards a stale late response", async () => {
    const first = deferred<SqlValidation>();
    const second = deferred<SqlValidation>();
    vi.mocked(validateQuery).mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    const { result, rerender } = renderHook(
      ({ sql }) => useSqlValidation("p1", "tab1", sql, null, 10),
      {
        initialProps: { sql: "SELECT * FROM missing" },
      },
    );
    await act(async () => {
      await vi.advanceTimersByTimeAsync(10);
    });
    expect(result.current.status).toBe("checking");

    rerender({ sql: "SELECT * FROM orders" });
    expect(result.current.status).toBe("editing");
    expect(result.current.diagnostics).toEqual([]);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(10);
    });
    await act(async () => first.resolve(problem(1)));
    expect(result.current.status).toBe("checking");
    expect(result.current.diagnostics).toEqual([]);

    await act(async () => second.resolve({ revision: 2, diagnostics: [] }));
    expect(result.current.status).toBe("clean");
  });

  it("rejects a response with the wrong revision and reports current failures non-blockingly", async () => {
    vi.mocked(validateQuery)
      .mockResolvedValueOnce(problem(999))
      .mockRejectedValueOnce(new Error("engine unavailable"));
    const { result, rerender } = renderHook(
      ({ sql }) => useSqlValidation("p1", "tab1", sql, null, 10),
      {
        initialProps: { sql: "SELECT missing" },
      },
    );
    await act(async () => {
      await vi.advanceTimersByTimeAsync(10);
    });
    expect(result.current.status).toBe("checking");

    rerender({ sql: "SELECT another" });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(10);
    });
    expect(result.current.status).toBe("unavailable");
    expect(result.current.message).toContain("engine unavailable");
  });

  it("revalidates unchanged SQL when the catalog revision changes", async () => {
    vi.mocked(validateQuery).mockImplementation(async (_projectId, _sql, revision) => ({
      revision,
      diagnostics: [],
    }));
    const { rerender } = renderHook(
      ({ catalog }) => useSqlValidation("p1", "tab1", "SELECT * FROM fresh_table", catalog, 10),
      { initialProps: { catalog: 1 } },
    );
    await act(async () => {
      await vi.advanceTimersByTimeAsync(10);
    });
    expect(validateQuery).toHaveBeenCalledTimes(1);
    rerender({ catalog: 2 });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(10);
    });
    expect(validateQuery).toHaveBeenCalledTimes(2);
  });

  it("cancels the timer on unmount and stays idle for blank SQL", async () => {
    const { result, unmount } = renderHook(() => useSqlValidation("p1", "tab1", "   ", null, 10));
    expect(result.current.status).toBe("idle");
    unmount();
    await act(async () => {
      await vi.runAllTimersAsync();
    });
    expect(validateQuery).not.toHaveBeenCalled();
  });
});
