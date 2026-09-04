import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { loadQuerySession, saveQuerySession } from "../../lib/commands";
import { sessionIdForProject, useQueryTabs } from "./useQueryTabs";

vi.mock("../../lib/commands", () => ({
  loadQuerySession: vi.fn(),
  saveQuerySession: vi.fn(),
}));

describe("useQueryTabs", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
    vi.mocked(loadQuerySession).mockResolvedValue(null);
    vi.mocked(saveQuerySession).mockResolvedValue(undefined);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("uses a deterministic project-scoped session id", () => {
    expect(sessionIdForProject("project-42")).toBe("project-project-42-main");
  });

  it("restores ordered tabs and the active tab", async () => {
    vi.mocked(loadQuerySession).mockResolvedValue({
      id: sessionIdForProject("p1"),
      projectId: "p1",
      tabs: [
        { id: "a", title: "First", sqlText: "SELECT 1", position: 0, isActive: false },
        { id: "b", title: "Second", sqlText: "SELECT 2", position: 1, isActive: true },
      ],
    });

    const { result } = renderHook(() => useQueryTabs("p1"));
    await act(async () => Promise.resolve());

    expect(result.current.tabs.map((tab) => tab.id)).toEqual(["a", "b"]);
    expect(result.current.activeTabId).toBe("b");
    expect(result.current.tabs.every((tab) => !tab.dirty)).toBe(true);
  });

  it("adds, renames, duplicates, moves, and closes tabs", async () => {
    const { result } = renderHook(() => useQueryTabs("p1"));
    await act(async () => Promise.resolve());
    const firstId = result.current.tabs[0].id;

    act(() => {
      result.current.renameTab(firstId, "Orders");
      result.current.editSql(firstId, "SELECT * FROM orders");
      result.current.duplicateTab(firstId);
    });
    expect(result.current.tabs).toHaveLength(2);
    expect(result.current.tabs[1].title).toBe("Orders copy");

    const duplicateId = result.current.tabs[1].id;
    act(() => result.current.moveTab(duplicateId, -1));
    expect(result.current.tabs[0].id).toBe(duplicateId);

    act(() => result.current.closeTab(duplicateId));
    expect(result.current.tabs).toHaveLength(1);
    expect(result.current.tabs[0].id).toBe(firstId);
  });

  it("debounces and serializes session saves", async () => {
    let releaseFirst: (() => void) | undefined;
    vi.mocked(saveQuerySession)
      .mockImplementationOnce(
        () =>
          new Promise<void>((resolve) => {
            releaseFirst = resolve;
          }),
      )
      .mockResolvedValue(undefined);

    const { result } = renderHook(() => useQueryTabs("p1"));
    await act(async () => Promise.resolve());
    const tabId = result.current.tabs[0].id;

    act(() => result.current.editSql(tabId, "SELECT 1"));
    act(() => vi.advanceTimersByTime(400));
    expect(saveQuerySession).toHaveBeenCalledTimes(1);

    act(() => result.current.editSql(tabId, "SELECT 2"));
    act(() => vi.advanceTimersByTime(400));
    expect(saveQuerySession).toHaveBeenCalledTimes(1);

    await act(async () => {
      releaseFirst?.();
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(saveQuerySession).toHaveBeenCalledTimes(2);
    expect(vi.mocked(saveQuerySession).mock.calls[1][0].tabs[0].sqlText).toBe("SELECT 2");
  });

  it("reports persistence failures without clearing the dirty flag", async () => {
    vi.mocked(saveQuerySession).mockRejectedValue(new Error("disk full"));
    const { result } = renderHook(() => useQueryTabs("p1"));
    await act(async () => Promise.resolve());
    const tabId = result.current.tabs[0].id;

    act(() => result.current.editSql(tabId, "SELECT 1"));
    await act(async () => {
      vi.advanceTimersByTime(400);
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(result.current.saveError).toContain("disk full");
    expect(result.current.tabs[0].dirty).toBe(true);
  });

  it("flush saves the latest revision without a debounce delay", async () => {
    const { result } = renderHook(() => useQueryTabs("p1"));
    await act(async () => Promise.resolve());
    const tabId = result.current.tabs[0].id;

    act(() => result.current.editSql(tabId, "SELECT 42"));
    await act(async () => result.current.flush());

    expect(saveQuerySession).toHaveBeenCalledTimes(1);
    expect(vi.mocked(saveQuerySession).mock.calls[0][0].tabs[0].sqlText).toBe("SELECT 42");
    expect(result.current.saveError).toBeNull();
  });

  it("flush rejects when the latest draft cannot be saved", async () => {
    vi.mocked(saveQuerySession).mockRejectedValue(new Error("disk full"));
    const { result } = renderHook(() => useQueryTabs("p1"));
    await act(async () => Promise.resolve());
    const tabId = result.current.tabs[0].id;

    act(() => result.current.editSql(tabId, "SELECT 1"));
    await expect(
      act(async () => {
        await result.current.flush();
      }),
    ).rejects.toThrow("disk full");
    expect(result.current.tabs[0].dirty).toBe(true);
  });
});
