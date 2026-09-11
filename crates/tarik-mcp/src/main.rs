mod bridge;
mod profile;
mod server;

use rmcp::ServiceExt;
use server::TarikMcpServer;

#[derive(Debug)]
struct Options {
    profile: String,
    label: String,
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("tarik-mcp: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let options = parse_options(std::env::args().skip(1))?;
    let server = TarikMcpServer::new(options.profile, options.label);
    let _ = server.connect();
    let service = server
        .serve(rmcp::transport::stdio())
        .await
        .map_err(|error| format!("could not start MCP stdio transport: {error}"))?;
    service
        .waiting()
        .await
        .map_err(|error| format!("MCP stdio transport stopped: {error}"))?;
    Ok(())
}

fn parse_options(arguments: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut profile = "default".to_string();
    let mut label = "Local MCP client".to_string();
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--profile" => {
                profile = arguments
                    .next()
                    .ok_or_else(|| "--profile requires a value".to_string())?;
            }
            "--label" => {
                label = arguments
                    .next()
                    .ok_or_else(|| "--label requires a value".to_string())?;
            }
            "--help" | "-h" => {
                eprintln!(
                    "Usage: tarik-mcp [--profile NAME] [--label DISPLAY_NAME]\n\nTarik Desktop must be running with Agent Access enabled."
                );
                std::process::exit(0);
            }
            _ => return Err(format!("unknown argument: {argument}")),
        }
    }
    if label.trim().is_empty() || label.len() > tarik_agent_protocol::MAX_CLIENT_LABEL_BYTES {
        return Err("--label must contain 1-80 bytes".into());
    }
    // Resolve once here so invalid names fail before MCP initialization.
    let _ = profile::profile_directory(&profile)?;
    Ok(Options { profile, label })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bounded_profile_and_label() {
        let options = parse_options([
            "--profile".into(),
            "claude".into(),
            "--label".into(),
            "Claude Desktop".into(),
        ])
        .unwrap();
        assert_eq!(options.profile, "claude");
        assert_eq!(options.label, "Claude Desktop");
        assert!(parse_options(["--unknown".into()]).is_err());
    }
}
