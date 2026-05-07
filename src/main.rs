mod server;
mod templates;

use std::path::PathBuf;

use rmcp::{ServiceExt, transport::stdio};

use crate::server::PostmortemServer;

enum CliAction {
    Run(PathBuf),
    Help,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    match parse_cli_args(std::env::args().skip(1)) {
        Ok(CliAction::Help) => {
            println!("{}", usage());
            return Ok(());
        }
        Ok(CliAction::Run(vault)) => {
            let server = PostmortemServer::new(vault);
            server
                .ensure_storage()
                .map_err(|error| format!("failed to initialize vault storage: {error:?}"))?;

            let service = server.serve(stdio()).await?;
            service.waiting().await?;
            Ok(())
        }
        Err(error) => {
            eprintln!("{error}");
            eprintln!("{}", usage());
            std::process::exit(2);
        }
    }
}

fn parse_cli_args(args: impl IntoIterator<Item = String>) -> Result<CliAction, String> {
    let mut vault = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => return Ok(CliAction::Help),
            "--vault" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--vault requires a path value".to_string())?;
                vault = Some(PathBuf::from(value));
            }
            unknown => {
                return Err(format!("unknown argument: {unknown}"));
            }
        }
    }

    vault
        .map(CliAction::Run)
        .ok_or_else(|| "missing required --vault argument".to_string())
}

fn usage() -> &'static str {
    "Usage: living-postmortem-mcp --vault <path>"
}
