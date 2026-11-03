use emelino_cli::{config::Config, errors, frontend, upstream_adapter};

use std::ffi::OsString;
use std::path::{Path, PathBuf};

const HELP: &str = "emelino — Emela Flow → embedded C

Usage:
  emelino flow <source.emel> --config <config.json> --out-dir <directory>
  emelino build <source.emel> --config <config.json> --out-dir <directory>
  emelino build --input <program.backend.json> --config <program.config.json> --out-dir <directory>
  emelino build --flow <program.flow.json> --out-dir <directory>  (legacy minimal replay)

Source input uses ordinary public functions selected by the configuration.
flow writes the versioned backend input, replay configuration and inspection-only Flow dump.
build also writes embedded C, public headers and the artifact manifest.
Additional source files can be supplied by config.sources relative to the entry source directory.
";

enum Input {
    Source(PathBuf),
    Flow(PathBuf),
    Backend(PathBuf),
}

struct Options {
    input: Input,
    out_dir: PathBuf,
    emit_c: bool,
    config: Option<PathBuf>,
}

fn parse(args: impl Iterator<Item = OsString>) -> Result<Options, String> {
    let mut args = args;
    let emit_c = match args.next().as_deref() {
        Some(command) if command == "build" => true,
        Some(command) if command == "flow" => false,
        _ => return Err(format!("expected `flow` or `build`\n\n{HELP}")),
    };
    let mut input = None;
    let mut out_dir = None;
    let mut config = None;
    while let Some(arg) = args.next() {
        if arg == "--out-dir" {
            if out_dir.is_some() {
                return Err("--out-dir was specified more than once".into());
            }
            out_dir = Some(next_path(&mut args, "--out-dir")?);
        } else if arg == "--config" {
            if config.is_some() {
                return Err("--config was specified more than once".into());
            }
            config = Some(next_path(&mut args, "--config")?);
        } else if arg == "--input" {
            if !emit_c || input.is_some() {
                return Err("--input requires build and exactly one input".into());
            }
            input = Some(Input::Backend(next_path(&mut args, "--input")?));
        } else if arg == "--flow" {
            if !emit_c || input.is_some() {
                return Err("--flow requires `build` and exactly one input".into());
            }
            input = Some(Input::Flow(next_path(&mut args, "--flow")?));
        } else if arg.to_string_lossy().starts_with('-') {
            return Err(format!("unknown option: {}", arg.to_string_lossy()));
        } else if input.is_none() {
            input = Some(Input::Source(arg.into()));
        } else {
            return Err("expected exactly one input".into());
        }
    }
    let input = input.ok_or("missing input")?;
    if matches!(input, Input::Flow(_)) {
        if config.is_some() {
            return Err("legacy --flow replay does not accept --config".into());
        }
    } else if config.is_none() {
        return Err("--config is required for source and backend inputs".into());
    }
    Ok(Options {
        input,
        out_dir: out_dir.ok_or("missing --out-dir")?,
        emit_c,
        config,
    })
}

fn next_path(args: &mut impl Iterator<Item = OsString>, flag: &str) -> Result<PathBuf, String> {
    let value = args
        .next()
        .ok_or_else(|| format!("missing path for {flag}"))?;
    if value.is_empty() || value.to_string_lossy().starts_with('-') {
        return Err(format!("missing path for {flag}"));
    }
    Ok(value.into())
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))
}

fn write(directory: &Path, name: &str, contents: &str) -> Result<(), String> {
    let path = directory.join(name);
    std::fs::write(&path, contents).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

fn run(options: Options) -> Result<(), String> {
    if let Input::Flow(path) = &options.input {
        let artifact = emelino_compiler::from_json(&read(path)?)?;
        let c = emelino_compiler::compile(&artifact)?;
        std::fs::create_dir_all(&options.out_dir).map_err(|e| e.to_string())?;
        write(&options.out_dir, "program.c", &c.source)?;
        return write(&options.out_dir, "program.h", &c.header);
    }
    let config = Config::from_json(&read(options.config.as_deref().ok_or("missing --config")?)?)?;
    let backend_options = config.options()?;
    let (input, dump) = match &options.input {
        Input::Source(path) => {
            let pairs = config.export_pairs()?;
            let entry_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or("entry filename must be UTF-8")?;
            let directory = path.parent().unwrap_or(Path::new("."));
            let mut sources = vec![(entry_name.to_owned(), read(path)?)];
            for source in &config.sources {
                let module = Path::new(source);
                if module
                    .components()
                    .any(|part| !matches!(part, std::path::Component::Normal(_)))
                {
                    return Err(format!("source must be a relative module path: {source}"));
                }
                sources.push((source.clone(), read(&directory.join(module))?));
            }
            let supplied: Vec<_> = sources
                .iter()
                .map(|(name, text)| frontend::Source { name, text })
                .collect();
            let generated = frontend::generate_sources(&supplied, entry_name, path)?;
            let program = upstream_adapter::adapt(&generated.module, &pairs)?;
            config.validate_entry(&program)?;
            (
                emelino_compiler::BackendInput::new(program).map_err(errors)?,
                Some(generated.dump),
            )
        }
        Input::Backend(path) => {
            config.validate_replay()?;
            let input = emelino_compiler::BackendInput::from_json(&read(path)?).map_err(errors)?;
            config.validate_entry(input.program())?;
            (input, None)
        }
        Input::Flow(_) => unreachable!(),
    };
    // Compile and serialize before changing output files, including `flow`: the
    // saved contract must be accepted by the backend with these resource limits.
    let output = input.compile(&backend_options).map_err(errors)?;
    let json = input.to_json().map_err(errors)?;
    let replay = serde_json::to_string_pretty(&config.replay()).map_err(|e| e.to_string())?;
    for file in &output.files {
        if Path::new(&file.path)
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
        {
            return Err(format!("invalid generated path: {}", file.path));
        }
    }
    std::fs::create_dir_all(&options.out_dir)
        .map_err(|e| format!("cannot create {}: {e}", options.out_dir.display()))?;
    write(&options.out_dir, "program.backend.json", &json)?;
    write(&options.out_dir, "program.config.json", &replay)?;
    if let Some(dump) = dump {
        write(&options.out_dir, "program.flow.txt", &dump)?;
    }
    if options.emit_c {
        for file in output.files {
            let path = options.out_dir.join(&file.path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
            }
            write(&options.out_dir, &file.path, &file.contents)?;
        }
    }
    Ok(())
}

fn main() -> std::process::ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        print!("{HELP}");
        return std::process::ExitCode::SUCCESS;
    }
    if args.len() == 1 && args[0] == "--version" {
        println!(
            "emelino {} (emela {})",
            env!("CARGO_PKG_VERSION"),
            emelino_compiler::UPSTREAM_REVISION
        );
        return std::process::ExitCode::SUCCESS;
    }
    match parse(args.into_iter()).and_then(run) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("emelino: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
