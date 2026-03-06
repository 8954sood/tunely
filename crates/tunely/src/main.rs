use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use anyhow::{Context, bail};
use clap::{Parser, Subcommand};

const DEFAULT_RELAY_CONFIG: &str = "/etc/tunely/relay.yaml";

#[derive(Debug, Parser)]
#[command(
    name = "tunely",
    version,
    about = "Tunely unified CLI",
    disable_help_subcommand = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Relay {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
    Agent {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
    #[command(about = "Print this message or the help of the given subcommand(s)")]
    Help,
}

#[derive(Debug, Clone, Copy)]
enum Target {
    Relay,
    Agent,
}

impl Target {
    fn bin_name(self) -> &'static str {
        if cfg!(windows) {
            match self {
                Target::Relay => "relay-server.exe",
                Target::Agent => "agent.exe",
            }
        } else {
            match self {
                Target::Relay => "relay-server",
                Target::Agent => "agent",
            }
        }
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Relay { args }) => {
            let args = ensure_default_relay_config(args);
            let code = run_target(Target::Relay, args)?;
            std::process::exit(code);
        }
        Some(Commands::Agent { args }) => {
            let code = run_target(Target::Agent, args)?;
            std::process::exit(code);
        }
        Some(Commands::Help) => {
            print_main_help()?;
            Ok(())
        }
        None => {
            print_main_help()?;
            Ok(())
        }
    }
}

fn print_main_help() -> anyhow::Result<()> {
    println!("Tunely unified CLI");
    println!();
    println!("Usage: tunely [COMMAND]");
    println!();
    println!("Commands:");
    println!(
        "  relay  Run relay-server ({})",
        install_state_label(Target::Relay)
    );
    println!(
        "  agent  Run agent ({})",
        install_state_label(Target::Agent)
    );
    println!("  help   Print this message or the help of the given subcommand(s)");
    println!();
    println!("Options:");
    println!("  -h, --help     Print help");
    println!("  -V, --version  Print version");
    Ok(())
}

fn install_state_label(target: Target) -> &'static str {
    if detect_installed_path(target).is_some() {
        "installed"
    } else {
        "not installed"
    }
}

fn ensure_default_relay_config(args: Vec<OsString>) -> Vec<OsString> {
    ensure_default_relay_config_for_path(args, Path::new(DEFAULT_RELAY_CONFIG))
}

fn ensure_default_relay_config_for_path(args: Vec<OsString>, config_path: &Path) -> Vec<OsString> {
    if has_config_arg(&args) || !config_path.exists() {
        return args;
    }

    let mut out = Vec::with_capacity(args.len() + 2);
    out.push(OsString::from("--config"));
    out.push(config_path.as_os_str().to_owned());
    out.extend(args);
    out
}

fn has_config_arg(args: &[OsString]) -> bool {
    args.iter().any(|arg| {
        let Some(s) = arg.to_str() else {
            return false;
        };
        s == "--config" || s.starts_with("--config=")
    })
}

fn run_target(target: Target, args: Vec<OsString>) -> anyhow::Result<i32> {
    let mut last_not_found: Option<anyhow::Error> = None;

    for candidate in candidate_commands(target)? {
        match spawn_and_wait(&candidate, &args) {
            Ok(status) => return Ok(status_to_code(status)),
            Err(err) => {
                if is_not_found(&err) {
                    last_not_found = Some(err);
                    continue;
                }
                return Err(err);
            }
        }
    }

    if let Some(err) = last_not_found {
        return Err(err).with_context(|| {
            format!(
                "failed to find executable '{}' (check installation/PATH)",
                target.bin_name()
            )
        });
    }

    bail!("no candidate executable for {}", target.bin_name())
}

fn candidate_commands(target: Target) -> anyhow::Result<Vec<OsString>> {
    let mut out = Vec::new();

    let current = std::env::current_exe().context("failed to resolve current executable path")?;
    if let Some(dir) = current.parent() {
        out.push(path_candidate(dir.join(target.bin_name())));
    }

    out.push(path_candidate(
        PathBuf::from("/opt/tunely").join(target.bin_name()),
    ));
    out.push(OsString::from(target.bin_name()));

    Ok(out)
}

fn detect_installed_path(target: Target) -> Option<PathBuf> {
    let current = std::env::current_exe().ok();
    if let Some(path) = current
        .as_ref()
        .and_then(|p| p.parent())
        .map(|dir| dir.join(target.bin_name()))
        .filter(|p| p.is_file())
    {
        return Some(path);
    }

    let opt_path = PathBuf::from("/opt/tunely").join(target.bin_name());
    if opt_path.is_file() {
        return Some(opt_path);
    }

    find_in_path(target.bin_name())
}

fn find_in_path(bin_name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    let dirs = std::env::split_paths(&path_var);
    for dir in dirs {
        let full = dir.join(bin_name);
        if full.is_file() {
            return Some(full);
        }
    }
    None
}

fn path_candidate(path: PathBuf) -> OsString {
    path.into_os_string()
}

fn spawn_and_wait(candidate: &OsString, args: &[OsString]) -> anyhow::Result<ExitStatus> {
    Command::new(candidate)
        .args(args)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .with_context(|| format!("failed to run '{}'", candidate.to_string_lossy()))
}

fn is_not_found(err: &anyhow::Error) -> bool {
    err.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound)
    })
}

fn status_to_code(status: ExitStatus) -> i32 {
    status.code().unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::{ensure_default_relay_config_for_path, has_config_arg};
    use std::ffi::OsString;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn detect_config_arg() {
        assert!(has_config_arg(&[
            OsString::from("--config"),
            OsString::from("x.yaml")
        ]));
        assert!(has_config_arg(&[OsString::from("--config=/tmp/x.yaml")]));
        assert!(!has_config_arg(&[
            OsString::from("--listen"),
            OsString::from("0.0.0.0:8080")
        ]));
    }

    #[test]
    fn inject_default_config_for_relay() {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let path = PathBuf::from(format!("/tmp/tunely-relay-config-{ts}.yaml"));
        fs::write(&path, "listen: 0.0.0.0:8080").expect("write");

        let out = ensure_default_relay_config_for_path(
            vec![OsString::from("--listen"), OsString::from("0.0.0.0:8080")],
            &path,
        );

        assert_eq!(out[0], OsString::from("--config"));
        assert_eq!(out[1], path.as_os_str().to_owned());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn do_not_inject_if_config_present() {
        let out = ensure_default_relay_config_for_path(
            vec![OsString::from("--config"), OsString::from("/tmp/r.yaml")],
            Path::new("/tmp/does-not-matter.yaml"),
        );
        assert_eq!(out[0], OsString::from("--config"));
        assert_eq!(out[1], OsString::from("/tmp/r.yaml"));
        assert_eq!(out.len(), 2);
    }
}
