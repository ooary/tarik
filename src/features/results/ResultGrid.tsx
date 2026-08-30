import { useVirtualizer } from "@tanstack/react-virtual";
import { CopyIcon } from "@phosphor-icons/react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getResultPage, type ResultPageView } from "../../lib/commands";

const ROW_HEIGHT = 34;
const ROW_NUMBER_WIDTH = 68;
const COLUMN_WIDTH = 150;
const PAGE_ROWS = 500;

export interface ResultColumn {
  name: string;
  logicalType: string;
  nativeType: string;
  nullable: boolean;
}

/** Extract typed column metadata from a page response. */
export function parseColumns(raw: unknown): ResultColumn[] {
  if (!Array.isArray(raw)) return [];
  return raw.flatMap((column) => {
    if (typeof column !== "object" || column === null) return [];
    const record = column as Record<string, unknown>;
    return [
      {
        name: String(record.name ?? ""),
        logicalType: String(record.logicalType ?? "unknown"),
        nativeType: String(record.nativeType ?? ""),
        nullable: Boolean(record.nullable),
      },
    ];
  });
}

/** Cell text: nulls become NULL markers; everything else is its text form. */
export function formatValue(value: unknown): string {
  if (value === null || value === undefined) return "NULL";
  return String(value);
}

/**
 * Virtualized, page-backed data grid for one published result. Only visible
 * rows and columns exist in the DOM; page artifacts stay on disk in the
 * engine and a small decoded cache lives in the desktop process.
 */
export function ResultGrid({ resultId, rowTotal }: { resultId: string; rowTotal: number }) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const [page, setPage] = useState<ResultPageView | null>(null);
  const [load, setLoad] = useState<{ status: "loading" | "ready" | "error"; message?: string }>({
    status: "loading",
  });
  const [activeRow, setActiveRow] = useState(0);

  const columns = useMemo(() => parseColumns(page?.columns), [page]);
  const rows = useMemo(() => page?.rows ?? [], [page]);

  const rowVirtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    // TanStack starts before the ref is measured. A non-zero initial viewport
    // prevents the first paint from containing no data rows.
    initialRect: { width: 900, height: 240 },
    overscan: 8,
  });
  const columnVirtualizer = useVirtualizer({
    horizontal: true,
    count: columns.length + 1,
    getScrollElement: () => scrollRef.current,
    estimateSize: (index) => (index === 0 ? ROW_NUMBER_WIDTH : COLUMN_WIDTH),
    // Without this, the pre-measure virtual window may contain only the row
    // number column, which looks like the result has no data columns.
    initialRect: { width: 900, height: 240 },
    overscan: 4,
  });

  const goToOffset = useCallback(
    (offset: number) => {
      const bounded = Math.max(0, Math.min(offset, rowTotal - 1));
      setLoad({ status: "loading" });
      setPage(null);
      setActiveRow(0);
      getResultPage(resultId, bounded)
        .then((next) => {
          setPage(next);
          setLoad({ status: "ready" });
        })
        .catch((error: unknown) => {
          setLoad({
            status: "error",
            message: error instanceof Error ? error.message : String(error),
          });
        });
    },
    [resultId, rowTotal],
  );

  useEffect(() => {
    let cancelled = false;
    setLoad({ status: "loading" });
    setPage(null);
    getResultPage(resultId, 0)
      .then((next) => {
        if (cancelled) return;
        setPage(next);
        setLoad({ status: "ready" });
      })
      .catch((error: unknown) => {
        setLoad({
          status: "error",
          message: error instanceof Error ? error.message : String(error),
        });
      });
    return () => {
      cancelled = true;
    };
  }, [resultId]);

  function onKeyDown(event: React.KeyboardEvent<HTMLDivElement>) {
    if (rows.length === 0) return;
    if (event.key === "ArrowDown") {
      const next = Math.min(activeRow + 1, rows.length - 1);
      setActiveRow(next);
      rowVirtualizer.scrollToIndex(next);
      event.preventDefault();
    } else if (event.key === "ArrowUp") {
      const next = Math.max(0, activeRow - 1);
      setActiveRow(next);
      rowVirtualizer.scrollToIndex(next);
      event.preventDefault();
    } else if (event.key === "PageDown") {
      goToOffset((page?.offset ?? 0) + PAGE_ROWS);
      event.preventDefault();
    } else if (event.key === "PageUp") {
      goToOffset((page?.offset ?? 0) - PAGE_ROWS);
      event.preventDefault();
    } else if (event.key === "c" && (event.ctrlKey || event.metaKey)) {
      copyPage();
    }
  }

  function copyPage() {
    if (!page) return;
    const header = columns.map((column) => column.name).join("\t");
    const body = rows.map((row) => row.map((cell) => formatValue(cell)).join("\t")).join("\n");
    void navigator.clipboard.writeText(`${header}\n${body}`);
  }

  function copyRow(rowIndex: number) {
    const row = rows[rowIndex];
    if (!row) return;
    void navigator.clipboard.writeText(row.map((cell) => formatValue(cell)).join("\t"));
  }

  const measuredRows = rowVirtualizer.getVirtualItems();
  const measuredColumns = columnVirtualizer.getVirtualItems();
  // Before ResizeObserver reports the viewport, TanStack can temporarily
  // return no virtual items. Render a small bounded first-paint window instead
  // of showing only the row count/header.
  const virtualRows =
    measuredRows.length > 0
      ? measuredRows
      : rows.slice(0, 16).map((_, index) => ({
          index,
          key: index,
          size: ROW_HEIGHT,
          start: index * ROW_HEIGHT,
        }));
  const virtualColumns =
    measuredColumns.length > 0
      ? measuredColumns
      : Array.from({ length: Math.min(columns.length + 1, 7) }, (_, index) => ({
          index,
          key: index,
          size: index === 0 ? ROW_NUMBER_WIDTH : COLUMN_WIDTH,
          start: index === 0 ? 0 : ROW_NUMBER_WIDTH + (index - 1) * COLUMN_WIDTH,
        }));
  const pageCount = Math.max(1, Math.ceil(rowTotal / PAGE_ROWS));
  const currentPage = Math.floor((page?.offset ?? 0) / PAGE_ROWS);
  const gridWidth = columns.length * COLUMN_WIDTH + ROW_NUMBER_WIDTH;

  return (
    <div className="result-surface">
      <div className="result-toolbar">
        <span>
          {rows.length > 0
            ? `Rows ${(page?.offset ?? 0) + 1}-${(page?.offset ?? 0) + rows.length} of ${rowTotal.toLocaleString("en-US")}`
            : `0 rows of ${rowTotal.toLocaleString("en-US")}`}
        </span>
        <div className="result-page-controls">
          <button
            className="subtle-button"
            disabled={(page?.offset ?? 0) === 0}
            onClick={() => goToOffset((page?.offset ?? 0) - PAGE_ROWS)}
            type="button"
          >
            Previous page
          </button>
          <span>
            Page {currentPage + 1} of {pageCount}
          </span>
          <button
            className="subtle-button"
            disabled={page === null || page.offset + PAGE_ROWS >= rowTotal}
            onClick={() => goToOffset((page?.offset ?? 0) + PAGE_ROWS)}
            type="button"
          >
            Next page
          </button>
        </div>
        <button className="subtle-button" onClick={copyPage} type="button">
          <CopyIcon aria-hidden="true" size={13} weight="bold" /> Copy page
        </button>
      </div>
      {load.status === "error" ? (
        <div className="result-grid-error" role="alert">
          {load.message}
        </div>
      ) : (
        <div
          aria-label="Query results"
          className="result-grid"
          onKeyDown={onKeyDown}
          ref={scrollRef}
          role="grid"
          tabIndex={0}
        >
          <div
            className="result-grid-inner"
            style={{
              height: rowVirtualizer.getTotalSize() + ROW_HEIGHT,
              position: "relative",
              width: gridWidth,
            }}
          >
            <div
              className="result-grid-header"
              style={{ height: ROW_HEIGHT, position: "relative", width: gridWidth }}
            >
              <span className="result-grid-cell row-number" style={{ width: ROW_NUMBER_WIDTH }}>
                #
              </span>
              {virtualColumns.map((virtualColumn) => {
                if (virtualColumn.index === 0) return null;
                const column = columns[virtualColumn.index - 1];
                if (!column) return null;
                return (
                  <span
                    className="result-grid-cell result-grid-header-cell"
                    key={`${column.name}-${virtualColumn.index}`}
                    style={{
                      left: virtualColumn.start,
                      position: "absolute",
                      width: virtualColumn.size,
                    }}
                  >
                    {column.name}
                    <small className="column-type">{column.nativeType}</small>
                  </span>
                );
              })}
            </div>
            {virtualRows.map((virtualRow) => {
              const rowIndex = virtualRow.index;
              const row = rows[rowIndex];
              if (!row) return null;
              return (
                <div
                  className={`result-grid-row ${rowIndex === activeRow ? "result-row-active" : ""}`}
                  key={virtualRow.key}
                  onClick={() => setActiveRow(rowIndex)}
                  onDoubleClick={() => copyRow(rowIndex)}
                  role="row"
                  style={{
                    height: ROW_HEIGHT,
                    left: 0,
                    position: "absolute",
                    top: ROW_HEIGHT + virtualRow.start,
                    width: gridWidth,
                  }}
                >
                  {virtualColumns.map((virtualColumn) => {
                    if (virtualColumn.index === 0) {
                      return (
                        <span
                          className="result-grid-cell row-number"
                          key="row-number"
                          style={{
                            left: virtualColumn.start,
                            position: "absolute",
                            width: virtualColumn.size,
                          }}
                        >
                          {(page?.offset ?? 0) + rowIndex + 1}
                        </span>
                      );
                    }
                    const columnIndex = virtualColumn.index - 1;
                    const column = columns[columnIndex];
                    if (!column) return null;
                    const isTruncated = (page?.truncatedCells ?? []).some(
                      ([cellRow, cellColumn]) =>
                        cellRow === rowIndex && cellColumn === virtualColumn.index - 1,
                    );
                    return (
                      <span
                        className={`result-grid-cell ${row[columnIndex] === null ? "cell-null" : ""}`}
                        key={column.name}
                        style={{
                          left: virtualColumn.start,
                          position: "absolute",
                          width: virtualColumn.size,
                        }}
                      >
                        {formatValue(row[columnIndex])}
                        {isTruncated ? "..." : ""}
                      </span>
                    );
                  })}
                </div>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}
