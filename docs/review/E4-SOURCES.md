# E4 local sources review

## Available workflow

With a DuckDB project open, **Import file** now opens the native file picker for CSV and Parquet.

### CSV

CSV always defaults to **Import as table**. The wizard provides:

- A maximum 25-row preview
- Inferred column names and DuckDB types
- Delimiter
- Header toggle
- Null representation
- Read-all-as-text toggle
- Destination table name
- Optional per-column DuckDB type overrides
- Import cancellation

The copy runs in a DuckDB transaction. Failure or interruption rolls back the incomplete table.

### Parquet

Parquet defaults to **Link file**, which creates a DuckDB view without copying data. The wizard also allows **Import as table**.

- Linked Parquet requires the original file to stay in place.
- Imported Parquet remains queryable if the original file moves.
- Linked paths, identifiers containing quotes/spaces, and multi-file globs are escaped and tested.

## Missing linked files

When a project opens, Tarik checks persisted linked paths. Missing links appear under **Linked sources** with a `Missing` state.

- Click a missing row or right-click and select **Locate replacement**.
- Replacement Parquet column names must match the original linked schema.
- Right-click and select **Remove link** to remove the DuckDB view and Tarik metadata without deleting the Parquet file.
- A broken link does not prevent the project or unrelated tables from opening.

## Manual review

```bash
npm run tauri:dev:clean
```

1. Create or open a DuckDB project.
2. Select **Import file** and choose a small CSV.
3. Change a CSV parse option and confirm the preview refreshes.
4. Confirm the schema and import it.
5. Verify the real table and column total appear in Explorer.
6. Choose a Parquet file and keep the default **Link file**.
7. Verify a view and linked-source record appear.
8. Close Tarik, move the Parquet file, and reopen the project.
9. Verify the source displays `Missing`, then locate a compatible replacement.
10. Repeat Parquet selection with **Import as table**, move the original file, and verify the table remains in DuckDB.

SQL editing and general query execution remain E5/E6.
