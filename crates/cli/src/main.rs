use clap::{Parser, Subcommand};
use reqchain_core::{
    cache::TokenCache,
    chain::Executor,
    exec::Runner,
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
        /// Print the effective request as a shell command instead of sending it
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
        Cmd::Run { api, endpoint, env, print_command } =>
            run(&paths, &api, &endpoint, env.as_deref(), print_command).await,
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
    if ws.errors.is_empty() { ExitCode::SUCCESS } else { ExitCode::from(1) }
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
            let label = match d.severity { Severity::Error => "error", Severity::Warning => "warning" };
            println!("{}: {}: {} at {}", file.display(), label, d.message, d.path);
        }
        if diags.iter().any(|d| d.severity == Severity::Error) {
            failed = true;
        } else {
            println!("{}: ok", file.display());
        }
    }
    if failed { ExitCode::from(1) } else { ExitCode::SUCCESS }
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
            ws.apis.iter().map(|a| a.id.as_str()).collect::<Vec<_>>().join(", ")
        );
        return ExitCode::from(3);
    };
    if api.endpoint(endpoint_id).is_none() {
        eprintln!(
            "error: endpoint `{endpoint_id}` not found in `{api_id}`. Available: {}",
            api.endpoints.iter().map(|e| e.id.as_str()).collect::<Vec<_>>().join(", ")
        );
        return ExitCode::from(3);
    }

    let secrets = Secrets::load(paths);
    let secret_values: Vec<String> = secrets.values().cloned().collect();
    let mut executor = Executor::new(Runner::new(), TokenCache::persistent(paths), secrets);

    match executor.run(api, endpoint_id, env).await {
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(if e.to_string().contains("transport") { 2 } else { 1 })
        }
        Ok(res) => {
            if let Err(e) = executor.cache_mut().save() {
                eprintln!(
                    "warning: could not save the token cache at {}: {e}",
                    paths.cache_file().display()
                );
            }

            // The token derived by chained auth is itself a credential and must be
            // masked alongside the literal secrets from the store — it appears in
            // the effective request and in the auth trace.
            let mut mask_values = secret_values;
            mask_values.extend(executor.derived_values());

            let masked = res.effective.masked(&mask_values);
            if print_command {
                println!("{}", shell::to_shell_command(&masked));
                return ExitCode::SUCCESS;
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
