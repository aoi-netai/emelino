mod commands;
mod firmware;
mod lsp;
mod project;
mod scaffold;

use std::ffi::OsString;
use std::path::Path;

const HELP: &str = "emelino — Emela applications for microcontrollers

Usage:
  emelino new <directory> --board <board> [--name <name>]
  emelino build [directory] [--emit c|firmware] [--cc <executable>] [--sdk-dir <directory>]
  emelino boards
  emelino lsp

Project builds discover Emelino.toml and default to complete firmware generation.
--emit c generates C without Python, a cross compiler or an SDK.

Explicit compiler inputs (C generation only):
  emelino flow <source.emel> --config <config.json> --out-dir <directory>
  emelino build <source.emel> --config <config.json> --out-dir <directory>
  emelino build --input <program.backend.json> --config <program.config.json> --out-dir <directory>
  emelino build --flow <program.flow.json> --out-dir <directory>  (legacy minimal replay)

flow writes backend input, replay configuration and an inspection-only Flow dump.
Explicit build also writes C, public headers and an artifact manifest.
Additional sources in explicit mode are listed in config.sources relative to the entry.
Use --help with a subcommand for details; --version prints compiler versions.
";

fn run(args: Vec<OsString>) -> Result<(), String> {
    if args.is_empty() || (args.len() == 1 && (args[0] == "--help" || args[0] == "-h")) {
        print!("{HELP}");
        return Ok(());
    }
    if args.len() == 1 && args[0] == "--version" {
        println!(
            "emelino {} (emela {})",
            env!("CARGO_PKG_VERSION"),
            emelino_compiler::UPSTREAM_REVISION
        );
        return Ok(());
    }
    if args.len() == 2 && (args[1] == "--help" || args[1] == "-h") {
        match args[0].to_str() {
            Some("new") => println!("{}", commands::new::HELP),
            Some("lsp") => println!("{}", lsp::HELP),
            Some("build") => println!("{}\n\n{HELP}", commands::project_build::HELP),
            Some("flow" | "boards") => print!("{HELP}"),
            _ => {
                return Err(format!(
                    "unknown command: {}\n\n{HELP}",
                    args[0].to_string_lossy()
                ));
            }
        }
        return Ok(());
    }
    match args[0].to_str() {
        Some("new") => commands::new::run(args.into_iter().skip(1)),
        Some("lsp") if args.len() == 1 => lsp::run_stdio(),
        Some("boards") if args.len() == 1 => {
            for board in firmware::boards() {
                println!("{}\t{}", board.board, board.physical_board);
            }
            Ok(())
        }
        Some("build") => {
            let explicit = args.iter().skip(1).any(|arg| {
                ["--config", "--out-dir", "--input", "--flow"]
                    .iter()
                    .any(|flag| arg == flag)
                    || (Path::new(arg).extension().is_some_and(|ext| ext == "emel")
                        && !Path::new(arg).is_dir())
            });
            if explicit {
                commands::compile::parse(args.into_iter()).and_then(commands::compile::run)
            } else {
                commands::project_build::run(args.into_iter().skip(1))
            }
        }
        Some("flow") => commands::compile::parse(args.into_iter()).and_then(commands::compile::run),
        _ => Err(format!(
            "unknown command: {}\n\n{HELP}",
            args[0].to_string_lossy()
        )),
    }
}

fn main() -> std::process::ExitCode {
    match run(std::env::args_os().skip(1).collect()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("emelino: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
