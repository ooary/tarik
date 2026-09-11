use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use serde_json::{json, Value};

struct McpProcess {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

impl McpProcess {
    fn start(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("tarik-mcp-stdio-{name}-{}", uuid::Uuid::new_v4()));
        let mut child = Command::new(env!("CARGO_BIN_EXE_tarik-mcp"))
            .args(["--profile", name, "--label", "Protocol test"])
            .env("TARIK_MCP_CONFIG_DIR", root.join("profiles"))
            .env("TARIK_AGENT_DIR", root.join("missing-agent"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn tarik-mcp");
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            input,
            output,
        }
    }

    fn send(&mut self, value: Value) {
        serde_json::to_writer(&mut self.input, &value).unwrap();
        self.input.write_all(b"\n").unwrap();
        self.input.flush().unwrap();
    }

    fn response(&mut self) -> Value {
        let mut line = String::new();
        self.output.read_line(&mut line).unwrap();
        assert!(!line.is_empty(), "tarik-mcp closed before responding");
        serde_json::from_str(&line).expect("stdout must contain JSON-RPC only")
    }

    fn initialize(&mut self, version: &str) -> Value {
        self.send(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": version,
                "capabilities": {},
                "clientInfo": { "name": "test", "version": "1" }
            }
        }));
        self.response()
    }

    fn finish(mut self) -> (String, String) {
        drop(self.input);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if self.child.try_wait().unwrap().is_some() {
                break;
            }
            if Instant::now() >= deadline {
                self.child.kill().unwrap();
                panic!("tarik-mcp did not stop after stdin EOF");
            }
            thread::sleep(Duration::from_millis(20));
        }
        let mut remaining_stdout = String::new();
        self.output.read_to_string(&mut remaining_stdout).unwrap();
        let mut stderr = String::new();
        BufReader::new(self.child.stderr.take().unwrap())
            .read_to_string(&mut stderr)
            .unwrap();
        assert!(self.child.wait().unwrap().success());
        (remaining_stdout, stderr)
    }
}

use std::io::Read;

#[test]
fn initializes_lists_static_tools_reports_unavailable_and_exits_on_eof() {
    let mut process = McpProcess::start("unavailable");
    let initialized = process.initialize("2025-11-25");
    assert_eq!(initialized["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(initialized["result"]["serverInfo"]["name"], "tarik-mcp");

    process.send(json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    }));
    process.send(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    }));
    let tools = process.response();
    let tools = tools["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 21);
    let names = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(names.contains(&"tarik_classify_sql"));
    assert!(names.contains(&"tarik_query_start"));
    assert!(names.contains(&"tarik_result_page"));
    assert!(names.contains(&"tarik_list_export_destinations"));
    assert!(names.contains(&"tarik_approval_status"));
    assert!(names.contains(&"tarik_execute_approved"));
    assert!(!names.contains(&"tarik_approve"));
    assert!(!names.iter().any(|name| {
        name.contains("create_export_destination")
            || name.contains("repair_export_destination")
            || name.contains("revoke_export_destination")
    }));
    let destinations = tools
        .iter()
        .find(|tool| tool["name"] == "tarik_list_export_destinations")
        .unwrap();
    assert_eq!(
        destinations["inputSchema"]["required"],
        json!(["projectId"])
    );
    assert!(destinations["inputSchema"]["properties"]
        .get("path")
        .is_none());

    process.send(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "prompts/list",
        "params": {}
    }));
    let prompts = process.response();
    let prompts = prompts["result"]["prompts"].as_array().unwrap();
    assert_eq!(prompts.len(), 7);
    let prompt_names = prompts
        .iter()
        .map(|prompt| prompt["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(prompt_names.contains(&"tarik-getting-started"));
    assert!(prompt_names.contains(&"tarik-full-query-export"));

    process.send(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "prompts/get",
        "params": { "name": "tarik-full-query-export" }
    }));
    let prompt = process.response();
    let text = prompt["result"]["messages"][0]["content"]["text"]
        .as_str()
        .unwrap();
    assert!(text.contains("complete immutable SafeRead query"));
    assert!(text.contains("5,000-row browse cap"));
    assert!(text.contains("untrusted content"));
    assert!(text.contains("Never provide or request a path"));

    process.send(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "prompts/get",
        "params": { "name": "not-a-tarik-prompt" }
    }));
    let missing = process.response();
    assert_eq!(missing["error"]["message"], "Unknown Tarik prompt");

    process.send(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": {
            "name": "tarik_server_info",
            "arguments": { "refresh": true }
        }
    }));
    let status = process.response();
    assert_eq!(status["result"]["structuredContent"]["available"], false);
    assert_eq!(status["result"]["isError"], false);

    let (stdout, stderr) = process.finish();
    assert!(
        stdout.is_empty(),
        "unexpected stdout after responses: {stdout:?}"
    );
    assert!(
        stderr.is_empty(),
        "unexpected stderr on healthy EOF: {stderr:?}"
    );
}

#[test]
fn negotiates_the_reviewed_2025_06_18_version() {
    let mut process = McpProcess::start("old-version");
    let initialized = process.initialize("2025-06-18");
    assert_eq!(initialized["result"]["protocolVersion"], "2025-06-18");
    let _ = process.finish();
}

#[test]
fn falls_unreviewed_handshake_versions_back_to_the_latest_reviewed_version() {
    let mut process = McpProcess::start("future-version");
    let initialized = process.initialize("2026-07-28");
    assert_eq!(initialized["result"]["protocolVersion"], "2025-11-25");
    let _ = process.finish();
}
