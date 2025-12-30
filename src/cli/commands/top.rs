use crate::android::AdbClient;
use crate::config::GlobalConfigManager;
use crate::core::error::{FridaMgrError, Result};
use crate::python::VenvExecutor;
use colored::Colorize;
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
    if let Some(arg) = args
        .iter()
        .find(|a| FORBIDDEN_FRIDA_ARGS.contains(&a.as_str()))
    {
        return Err(FridaMgrError::Config(format!(
            "frida-mgr top selects the device and target automatically; do not pass '{}'. Use 'frida-mgr frida' for full control.",
            arg
        )));
    }

    let global_config = GlobalConfigManager::new()?.load().await?;
    let adb = AdbClient::new(Some(global_config.android.adb_path));
    let device = adb.get_device(device_id.as_deref()).await?;

    let foreground = adb.get_foreground_app(&device.id).await?;

    println!(
        "{} Foreground: {} ({})",
        "ℹ".blue().bold(),
        foreground.package.cyan(),
        foreground.process.yellow()
    );
    if let Some(pid) = foreground.pid {
        println!("  PID: {}", pid.to_string().yellow());
    }
    if let Some(activity) = foreground.activity.as_deref() {
        println!("  Activity: {}", activity.cyan());
    }

    let mut frida_args = Vec::with_capacity(8 + scripts.len() * 2 + args.len());
    frida_args.push("-D".to_string());
    frida_args.push(device.id);
    if let Some(pid) = foreground.pid {
        frida_args.push("-p".to_string());
        frida_args.push(pid.to_string());
    } else {
        frida_args.push("-n".to_string());
        frida_args.push(foreground.process);
    }

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
