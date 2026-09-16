use clap::{Parser, Subcommand};
use reqchain_core::{
    cache::TokenCache,
    chain::{Executor, RunError},
    exec::{ExecError, Runner},
    paths::Paths,
    secrets::Secrets,
    shell,
    store::Workspace,
    validate::{self, Severity},
};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "reqchain", about = "HTTP client with request-derived auth")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List APIs and their endpoints
    List,
    /// Run an endpoint, resolving its auth chain
    Run {
        api: String,
        endpoint: String,
        #[arg(long)]
        env: Option<String>,
        /// Print the effective request as a shell command instead of sending it.
        /// The endpoint's own request is never sent; a chained auth still calls its
        /// auth endpoint, because the token does not exist until it does.
        #[arg(long = "print-command")]
        print_command: bool,
    },
    /// Validate API files (defaults to every file in the workspace)
    Validate { files: Vec<PathBuf> },
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let paths = Paths::from_env();
    match cli.command {
        Cmd::List => list(&paths),
        Cmd::Validate { files } => validate_cmd(&paths, files),
        Cmd::Run {
            api,
            endpoint,
            env,
            print_command,
        } => run(&paths, &api, &endpoint, env.as_deref(), print_command).await,
    }
}

fn list(paths: &Paths) -> ExitCode {
    let ws = Workspace::load(paths);
    for api in &ws.apis {
        println!("{} — {} ({})", api.id, api.name, api.base_url);
        for ep in &api.endpoints {
            println!("  {:<24} {:?} {}", ep.id, ep.method, ep.path);
        }
    }
    for err in &ws.errors {
        eprintln!("error: {}: {}", err.path.display(), err.message);
    }
    if ws.errors.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn validate_cmd(paths: &Paths, files: Vec<PathBuf>) -> ExitCode {
    let targets: Vec<PathBuf> = if files.is_empty() {
        std::fs::read_dir(paths.apis_dir())
            .map(|d| {
                d.flatten()
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|x| x == "json"))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        files
    };

    let mut failed = false;
    for file in targets {
        let Ok(text) = std::fs::read_to_string(&file) else {
            eprintln!("error: cannot read {}", file.display());
            failed = true;
            continue;
        };
        let diags = validate::validate_text(&text);
        for d in &diags {
            let label = match d.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
            };
            println!("{}: {}: {} at {}", file.display(), label, d.message, d.path);
        }
        if diags.iter().any(|d| d.severity == Severity::Error) {
            failed = true;
        } else {
            println!("{}: ok", file.display());
        }
    }
    if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

async fn run(
    paths: &Paths,
    api_id: &str,
    endpoint_id: &str,
    env: Option<&str>,
    print_command: bool,
) -> ExitCode {
    let ws = Workspace::load(paths);
    let Some(api) = ws.api(api_id) else {
        eprintln!(
            "error: API `{api_id}` not found. Available: {}",
            ws.apis
                .iter()
                .map(|a| a.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
        return ExitCode::from(3);
    };
    if api.endpoint(endpoint_id).is_none() {
        eprintln!(
            "error: endpoint `{endpoint_id}` not found in `{api_id}`. Available: {}",
            api.endpoints
                .iter()
                .map(|e| e.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
        return ExitCode::from(3);
    }

    let secrets = Secrets::load(paths);
    let secret_values: Vec<String> = secrets.values().cloned().collect();
    let mut executor = Executor::new(Runner::new(), TokenCache::persistent(paths), secrets);

    if print_command {
        // `--print-command` must NOT perform the endpoint's own request: it is
        // advertised as showing the request "instead of sending it", and a
        // DELETE endpoint printed this way must still be un-deleted afterwards.
        // Resolving a chained auth may legitimately call the AUTH endpoint —
        // the token does not exist until it has — and that is documented on the
        // flag and in the README.
        return match executor.prepare(api, endpoint_id, env).await {
            Err(e) => {
                let mut mask_values = secret_values;
                mask_values.extend(executor.derived_values());
                eprintln!("error: {}", mask_all(&e.to_string(), &mask_values));
                let is_transport = matches!(e, RunError::Exec(ExecError::Transport(_)));
                ExitCode::from(if is_transport { 2 } else { 1 })
            }
            Ok(req) => {
                let mut mask_values = secret_values;
                mask_values.extend(executor.derived_values());
                if let Err(e) = executor.cache_mut().save() {
                    eprintln!(
                        "warning: could not save the token cache at {}: {}",
                        paths.cache_file().display(),
                        mask_all(&e.to_string(), &mask_values)
                    );
                }
                println!("{}", shell::to_shell_command(&req.masked(&mask_values)));
                ExitCode::SUCCESS
            }
        };
    }

    match executor.run(api, endpoint_id, env).await {
        Err(e) => {
            // A chain error's message can legitimately embed up to 200 raw
            // characters of the failing auth endpoint's response body — that is
            // exactly where a leaked secret or a derived token would show up.
            // Mask before printing, using every value we know is sensitive so
            // far (literal secrets plus whatever the chain had already derived
            // before it failed).
            let mut mask_values = secret_values;
            mask_values.extend(executor.derived_values());
            eprintln!("error: {}", mask_all(&e.to_string(), &mask_values));

            // Classify on the error variant, not on its rendered text: a chain
            // failure can legitimately contain the word "transport" (e.g. an
            // auth endpoint replying with a body like
            // `{"error":"transport layer timeout"}`), which must not be
            // reported as a transport failure.
            let is_transport = matches!(e, RunError::Exec(ExecError::Transport(_)));
            ExitCode::from(if is_transport { 2 } else { 1 })
        }
        Ok(res) => {
            // The token derived by chained auth is itself a credential and must be
            // masked alongside the literal secrets from the store — it appears in
            // the effective request and in the auth trace.
            let mut mask_values = secret_values;
            mask_values.extend(executor.derived_values());

            if let Err(e) = executor.cache_mut().save() {
                eprintln!(
                    "warning: could not save the token cache at {}: {}",
                    paths.cache_file().display(),
                    mask_all(&e.to_string(), &mask_values)
                );
            }

            eprintln!("{} {}ms {}B", res.status, res.elapsed_ms, res.size_bytes);
            for step in &res.auth_trace {
                if step.from_cache {
                    eprintln!("auth: {} -> (cached)", step.endpoint_id);
                } else {
                    eprintln!("auth: {} -> {}", step.endpoint_id, step.status);
                }
            }
            let text = res.body_text();
            match serde_json::from_str::<serde_json::Value>(&text) {
                Ok(v) => println!("{}", serde_json::to_string_pretty(&v).unwrap_or(text)),
                Err(_) => println!("{text}"),
            }
            ExitCode::SUCCESS
        }
    }
}

/// Replaces every occurrence of any value in `mask` with `***`. Used to scrub
/// secrets and chain-derived tokens out of text we did not otherwise get a
/// chance to mask structurally, such as an error message.
fn mask_all(text: &str, mask: &[String]) -> String {
    let mut out = text.to_string();
    for value in mask {
        if !value.is_empty() {
            out = out.replace(value.as_str(), "***");
        }
    }
    out
}
