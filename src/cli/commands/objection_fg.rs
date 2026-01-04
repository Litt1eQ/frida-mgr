use crate::cli::commands::foreground::{ensure_no_forbidden_args, resolve_foreground_context};
use crate::core::error::Result;
use crate::python::VenvExecutor;
use std::env;

const FORBIDDEN_OBJECTION_ARGS: &[&str] = &[
    "-g", "--gadget", "-n", "--name", "-S", "--serial", "-d", "--device",
];

fn option_present(help: &str, short: &str, long: &str) -> bool {
    let long_prefix = format!("--{long}");
    let short_long_prefix1 = format!("{short}, {long_prefix}");
    let short_long_prefix2 = format!("{short},{long_prefix}");

    help.lines().any(|line| {
        let line = line.trim_start();
        line.starts_with(&short_long_prefix1)
            || line.starts_with(&short_long_prefix2)
            || line.starts_with(&long_prefix)
    })
}

fn parse_objection_device_flag(help: &str) -> Option<&'static str> {
    if option_present(help, "-S", "serial") {
        Some("--serial")
    } else if option_present(help, "-d", "device") {
        Some("--device")
    } else {
        None
    }
}

fn parse_objection_target_flag(help: &str) -> Option<&'static str> {
    if option_present(help, "-n", "name") {
        Some("--name")
    } else if option_present(help, "-g", "gadget") {
        Some("--gadget")
    } else {
        None
    }
}

fn help_has_command(help: &str, command: &str) -> bool {
    help.lines().any(|line| {
        let trimmed = line.trim_start();
        if !trimmed.starts_with(command) {
            return false;
        }
        trimmed
            .chars()
            .nth(command.len())
            .map(|c| c.is_whitespace())
            .unwrap_or(true)
    })
}

fn parse_default_subcommand(help: &str) -> Option<&'static str> {
    if help_has_command(help, "start") {
        Some("start")
    } else if help_has_command(help, "explore") {
        Some("explore")
    } else {
        None
    }
}

struct ObjectionCliInfo {
    device_flag: Option<&'static str>,
    target_flag: Option<&'static str>,
    default_subcommand: Option<&'static str>,
}

fn parse_objection_cli_info(help: &str) -> ObjectionCliInfo {
    ObjectionCliInfo {
        device_flag: parse_objection_device_flag(help),
        target_flag: parse_objection_target_flag(help),
        default_subcommand: parse_default_subcommand(help),
    }
}

async fn detect_objection_cli_info(executor: &VenvExecutor) -> Option<ObjectionCliInfo> {
    let args = vec!["--help".to_string()];
    let output = executor.run_captured("objection", &args).await.ok()?;
    let help = format!("{}\n{}", output.stdout, output.stderr);
    Some(parse_objection_cli_info(&help))
}

pub async fn execute(device_id: Option<String>, args: Vec<String>) -> Result<()> {
    ensure_no_forbidden_args(
        &args,
        FORBIDDEN_OBJECTION_ARGS,
        "frida-mgr objection-fg selects the device and target automatically",
    )?;

    let current_dir = env::current_dir()?;
    let executor = VenvExecutor::new(current_dir);

    let foreground = resolve_foreground_context(device_id.as_deref()).await?;
    foreground.print_summary();

    let cli_info = detect_objection_cli_info(&executor).await;
    let Some(cli_info) = cli_info else {
        return Err(crate::core::error::FridaMgrError::PythonEnv(
            "Unable to inspect objection CLI; is it installed in the venv?".to_string(),
        ));
    };

    if device_id.is_some() && cli_info.device_flag.is_none() {
        return Err(crate::core::error::FridaMgrError::Config(
            "Unable to detect how to select a device for objection; use 'frida-mgr objection' for full control.".to_string(),
        ));
    }

    let Some(target_flag) = cli_info.target_flag else {
        return Err(crate::core::error::FridaMgrError::Config(
            "Unable to detect how to specify an app target for objection; use 'frida-mgr objection' for full control.".to_string(),
        ));
    };

    let mut objection_args =
        Vec::with_capacity(4 + cli_info.device_flag.map(|_| 2).unwrap_or(0) + args.len());
    if let Some(device_flag) = cli_info.device_flag {
        objection_args.push(device_flag.to_string());
        objection_args.push(foreground.device.id);
    }

    objection_args.push(target_flag.to_string());
    objection_args.push(foreground.package);

    if args.is_empty() {
        objection_args.push(cli_info.default_subcommand.unwrap_or("explore").to_string());
    } else {
        objection_args.extend(args);
    }

    let exit_code = executor
        .run_interactive("objection", &objection_args)
        .await?;
    std::process::exit(exit_code);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_serial_flag() {
        let help = "Options:\n  -S, --serial TEXT  Set device serial\n";
        assert_eq!(parse_objection_device_flag(help), Some("--serial"));
    }

    #[test]
    fn detects_device_fallback_flag() {
        let help = "Options:\n  -d, --device TEXT  Set device\n";
        assert_eq!(parse_objection_device_flag(help), Some("--device"));
    }

    #[test]
    fn returns_none_when_unknown() {
        let help = "Options:\n  --foo bar\n";
        assert_eq!(parse_objection_device_flag(help), None);
    }

    #[test]
    fn prefers_name_over_gadget() {
        let help = "Options:\n  -g, --gadget TEXT  (deprecated)\n  -n, --name TEXT  App name\n";
        assert_eq!(parse_objection_target_flag(help), Some("--name"));
    }

    #[test]
    fn falls_back_to_gadget_when_name_missing() {
        let help = "Options:\n  -g, --gadget TEXT  Gadget\n";
        assert_eq!(parse_objection_target_flag(help), Some("--gadget"));
    }

    #[test]
    fn does_not_false_positive_on_unrelated_n_flag() {
        let help = "Options:\n  -n, --no-color  Disable color\n  -g, --gadget TEXT  Gadget name\n";
        assert_eq!(parse_objection_target_flag(help), Some("--gadget"));
    }

    #[test]
    fn detects_start_as_default_subcommand() {
        let help = "Commands:\n  start    Start runtime\n  explore  Explore (deprecated)\n";
        assert_eq!(parse_default_subcommand(help), Some("start"));
    }

    #[test]
    fn detects_explore_when_start_missing() {
        let help = "Commands:\n  explore  Explore\n";
        assert_eq!(parse_default_subcommand(help), Some("explore"));
    }
}
