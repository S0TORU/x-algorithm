use anyhow::{anyhow, Context};
use serde_json::{json, Value};
use std::env;
use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::Command;

fn main() -> anyhow::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = stdout.lock();

    while let Some(msg) = read_message(&mut reader)? {
        if let Some(response) = handle_message(msg) {
            write_message(&mut writer, &response)?;
        }
    }

    Ok(())
}

fn handle_message(msg: Value) -> Option<Value> {
    let method = msg.get("method").and_then(Value::as_str)?;
    let id = msg.get("id").cloned();
    let params = msg.get("params").cloned().unwrap_or_else(|| json!({}));

    match method {
        "notifications/initialized" => None,
        "initialize" => Some(success(
            id,
            json!({
                "protocolVersion": params
                    .get("protocolVersion")
                    .and_then(Value::as_str)
                    .unwrap_or("2025-11-05"),
                "capabilities": {
                    "tools": {
                        "listChanged": false
                    }
                },
                "serverInfo": {
                    "name": "sentinelpipe-mcp",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        )),
        "ping" => Some(success(id, json!({}))),
        "tools/list" => Some(success(id, json!({ "tools": tool_list() }))),
        "tools/call" => Some(handle_tool_call(id, params)),
        _ => id.map(|rid| error(rid, -32601, &format!("unknown method: {}", method))),
    }
}

fn tool_list() -> Vec<Value> {
    vec![
        json!({
            "name": "redteam_preview",
            "description": "Preview SentinelPipe scenarios from a run config without executing the target.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "config_path": {"type": "string"},
                    "show_prompts": {"type": "boolean"}
                },
                "required": ["config_path"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "redteam_run",
            "description": "Run SentinelPipe against a configured target and return deterministic findings.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "config_path": {"type": "string"},
                    "top_k": {"type": "integer"},
                    "details": {"type": "boolean"}
                },
                "required": ["config_path"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "redteam_doctor",
            "description": "Check that the configured target endpoint is reachable and the model appears available.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "config_path": {"type": "string"}
                },
                "required": ["config_path"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "redteam_batch",
            "description": "Run multiple SentinelPipe configs and report pass/fail deltas across candidates.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "config_paths": {
                        "type": "array",
                        "items": {"type": "string"},
                        "minItems": 1
                    },
                    "top_k": {"type": "integer"},
                    "details": {"type": "boolean"}
                },
                "required": ["config_paths"],
                "additionalProperties": false
            }
        }),
    ]
}

fn handle_tool_call(id: Option<Value>, params: Value) -> Value {
    let name = match params.get("name").and_then(Value::as_str) {
        Some(name) => name,
        None => return error(id.unwrap_or(Value::Null), -32602, "tools/call missing name"),
    };
    let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));

    let result = match name {
        "redteam_preview" => call_preview(args),
        "redteam_run" => call_run(args),
        "redteam_doctor" => call_doctor(args),
        "redteam_batch" => call_batch(args),
        _ => Err(anyhow!("unknown tool: {}", name)),
    };

    match result {
        Ok((value, ok, stderr)) => success(
            id,
            json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string())
                }],
                "structuredContent": value,
                "isError": !ok,
                "_meta": {
                    "stderr": stderr
                }
            }),
        ),
        Err(err) => error(id.unwrap_or(Value::Null), -32000, &err.to_string()),
    }
}

fn call_preview(args: Value) -> anyhow::Result<(Value, bool, String)> {
    let config_path = required_string(&args, "config_path")?;
    let mut cli_args = vec!["dry-run".to_string(), "--config".to_string(), config_path, "--json".to_string()];
    if args.get("show_prompts").and_then(Value::as_bool).unwrap_or(false) {
        cli_args.push("--show-prompts".to_string());
    }
    call_cli_json(&cli_args)
}

fn call_run(args: Value) -> anyhow::Result<(Value, bool, String)> {
    let config_path = required_string(&args, "config_path")?;
    let mut cli_args = vec!["run".to_string(), "--config".to_string(), config_path, "--json".to_string()];
    if let Some(top_k) = args.get("top_k").and_then(Value::as_u64) {
        cli_args.push("--top-k".to_string());
        cli_args.push(top_k.to_string());
    }
    if args.get("details").and_then(Value::as_bool).unwrap_or(false) {
        cli_args.push("--details".to_string());
    }
    call_cli_json(&cli_args)
}

fn call_doctor(args: Value) -> anyhow::Result<(Value, bool, String)> {
    let config_path = required_string(&args, "config_path")?;
    call_cli_json(&[
        "doctor".to_string(),
        "--config".to_string(),
        config_path,
        "--json".to_string(),
    ])
}

fn call_batch(args: Value) -> anyhow::Result<(Value, bool, String)> {
    let config_paths = args
        .get("config_paths")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("config_paths must be an array"))?;
    let mut cli_args = vec!["batch".to_string()];
    for path in config_paths {
        let path = path
            .as_str()
            .ok_or_else(|| anyhow!("config_paths must contain strings"))?;
        cli_args.push("--config".to_string());
        cli_args.push(path.to_string());
    }
    if let Some(top_k) = args.get("top_k").and_then(Value::as_u64) {
        cli_args.push("--top-k".to_string());
        cli_args.push(top_k.to_string());
    }
    if args.get("details").and_then(Value::as_bool).unwrap_or(false) {
        cli_args.push("--details".to_string());
    }
    cli_args.push("--json".to_string());
    call_cli_json(&cli_args)
}

fn required_string(args: &Value, key: &str) -> anyhow::Result<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("{} is required", key))
}

fn call_cli_json(args: &[String]) -> anyhow::Result<(Value, bool, String)> {
    let cli = resolve_cli_path()?;
    let output = Command::new(cli)
        .args(args)
        .output()
        .with_context(|| format!("failed to invoke sentinelpipe-cli with args {:?}", args))?;

    let stdout = String::from_utf8(output.stdout).context("cli stdout was not utf-8")?;
    let stderr = String::from_utf8(output.stderr).unwrap_or_else(|_| String::new());
    let value = serde_json::from_str::<Value>(&stdout)
        .with_context(|| format!("cli did not emit valid json. stdout={} stderr={}", stdout, stderr))?;
    Ok((value, output.status.success(), stderr))
}

fn resolve_cli_path() -> anyhow::Result<PathBuf> {
    if let Ok(path) = env::var("SENTINELPIPE_CLI") {
        return Ok(PathBuf::from(path));
    }

    let exe = env::current_exe().context("failed to resolve current executable")?;
    if let Some(parent) = exe.parent() {
        let sibling = parent.join("sentinelpipe-cli");
        if sibling.exists() {
            return Ok(sibling);
        }
    }

    Ok(PathBuf::from("sentinelpipe-cli"))
}

fn success(id: Option<Value>, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "result": result,
    })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message,
        }
    })
}

fn read_message<R: BufRead>(reader: &mut R) -> anyhow::Result<Option<Value>> {
    let mut content_length: Option<usize> = None;
    let mut line = String::new();

    loop {
        line.clear();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            return Ok(None);
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length:") {
            content_length = Some(value.trim().parse::<usize>().context("invalid Content-Length")?);
        }
    }

    let len = content_length.ok_or_else(|| anyhow!("missing Content-Length header"))?;
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body)?;
    let value = serde_json::from_slice::<Value>(&body).context("invalid json-rpc payload")?;
    Ok(Some(value))
}

fn write_message<W: Write>(writer: &mut W, message: &Value) -> anyhow::Result<()> {
    let body = serde_json::to_vec(message)?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()?;
    Ok(())
}
