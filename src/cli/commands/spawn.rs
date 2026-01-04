use crate::cli::commands::foreground::{ensure_no_forbidden_args, resolve_foreground_context};
use crate::core::error::Result;
use crate::python::VenvExecutor;
use std::env;

const FORBIDDEN_FRIDA_ARGS: &[&str] = &[
    "-U",
    "--usb",
    "-D",
    "--device",
    "-H",
    "--host",
    "-n",
    "--attach-name",
    "-p",
    "--attach-pid",
    "-f",
    "--spawn",
    "-F",
    "--attach-frontmost",
];

pub async fn execute(
    device_id: Option<String>,
    scripts: Vec<String>,
    args: Vec<String>,
) -> Result<()> {
    ensure_no_forbidden_args(
        &args,
        FORBIDDEN_FRIDA_ARGS,
        "frida-mgr spawn selects the device and target automatically",
    )?;

    let foreground = resolve_foreground_context(device_id.as_deref()).await?;
    foreground.print_summary();

    let mut frida_args = Vec::with_capacity(8 + scripts.len() * 2 + args.len());
    frida_args.push("-D".to_string());
    frida_args.push(foreground.device.id);
    frida_args.push("-f".to_string());
    frida_args.push(foreground.package);

    for script in scripts {
        frida_args.push("-l".to_string());
        frida_args.push(script);
    }

    frida_args.extend(args);

    let current_dir = env::current_dir()?;
    let executor = VenvExecutor::new(current_dir);
    let exit_code = executor.run_interactive("frida", &frida_args).await?;

    std::process::exit(exit_code);
}
