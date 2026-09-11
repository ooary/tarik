# Connect an MCP client to Tarik

Tarik exposes a local, model-independent MCP server. The MCP host starts `tarik-mcp` as a background stdio child. Tarik Desktop must already be running with **Agent access** enabled; no TCP listener, cloud account, model credential, Node.js, Python, Rust, or separate DuckDB installation is required in a packaged release.

## Before connecting

1. Open Tarik Desktop and open the project the agent may use.
2. Open **Agent access** from the header and choose **Enable**.
3. Configure the MCP host with an absolute path to the packaged `tarik-mcp` binary.
4. Start or restart the MCP host. A pending pairing appears in Tarik.
5. Confirm the displayed client and select **Pair client**.
6. Grant only the required capabilities for the active project. `Inspect catalog` and `Analyze data` are the recommended starting grants.

A new client receives no project access. A granted but closed project can be listed, but it cannot be opened or queried by the agent. Closing Tarik, closing the project, disabling Agent Access, revoking the client, or terminating the MCP host invalidates active connection-owned work.

## Command

Linux portable archive:

```text
/absolute/path/Tarik-0.1.0-linux-x86_64/tarik-mcp --profile HOST_NAME --label "DISPLAY NAME"
```

Windows portable archive:

```text
C:\absolute\path\Tarik-0.1.0-windows-x64-portable\tarik-mcp.exe --profile HOST_NAME --label "DISPLAY NAME"
```

`--profile` accepts 1–64 ASCII letters, numbers, hyphens, or underscores. Use a different profile for each MCP-host configuration. Its private pairing credential is stored in the current user's configuration directory; never copy it between users or machines.

## Claude Desktop

Add a stdio server to the Claude Desktop MCP configuration and replace the path:

```json
{
  "mcpServers": {
    "tarik": {
      "command": "/absolute/path/to/tarik-mcp",
      "args": ["--profile", "claude-desktop", "--label", "Claude Desktop"]
    }
  }
}
```

On Windows, use the absolute `tarik-mcp.exe` path with escaped backslashes or forward slashes.

## Claude Code

Register the same stdio command using the current Claude Code MCP configuration command or project settings:

```text
tarik-mcp --profile claude-code --label "Claude Code"
```

The executable path must be absolute. Do not configure HTTP, SSE, OAuth, an API key, or a Tarik database path.

## Pi

Add the standard stdio MCP process to the Pi MCP extension/configuration in use:

```json
{
  "command": "/absolute/path/to/tarik-mcp",
  "args": ["--profile", "pi", "--label", "Pi"]
}
```

Pi starts and owns the child. Closing Pi closes stdin, and `tarik-mcp` exits cleanly.

## Cursor, VS Code, and generic MCP inspectors

Use their stdio MCP-server configuration with the same absolute command and arguments. Tarik supports initialization revisions `2025-06-18` and `2025-11-25`. A later requested handshake revision falls back to the latest reviewed revision rather than enabling newer capabilities.

## Safety model

- Project paths, source paths, logs, credentials, environment variables, and unrelated projects are never discovery output.
- SQL is classified before execution. Exactly one statement is required.
- Unknown syntax, unknown relations/functions/views/macros, external readers, URLs, extensions, secrets, raw file SQL, settings, calls, and transactions are blocked.
- Safe reads execute only from a one-use immutable snapshot and are capped at 5,000 rows, 500 rows per page, 1 MiB per page response, and 60 seconds.
- Mutations require a visible one-use approval inside Tarik. Critical destructive changes additionally require typing Tarik's generated phrase.
- MCP has no approval tool. Confirmation shown by an MCP host cannot replace direct Tarik approval.
- `tarik_execute_approved` accepts only an approval ID and runs Tarik's server-held snapshot; it never accepts resent SQL.
- Failure rows from Quality Checks are not persisted or exposed by the v1 MCP tools.

## Troubleshooting

Call `tarik_server_info` with `refresh: true`. Its guidance distinguishes unavailable desktop, pending pairing, missing project grant, and authenticated readiness.

If authentication fails after revocation or credential loss, remove that host profile's private `profile.json`, restart the MCP host, and pair it again in Tarik. Do not edit Tarik's bridge descriptor or place either file in a shared directory.
