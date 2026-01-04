use crate::android::{AdbClient, Device};
use crate::config::GlobalConfigManager;
use crate::core::error::{FridaMgrError, Result};
use colored::Colorize;

pub struct ForegroundContext {
    pub device: Device,
    pub package: String,
    pub process: String,
    pub pid: Option<u32>,
    pub activity: Option<String>,
}

impl ForegroundContext {
    pub fn print_summary(&self) {
        println!(
            "{} Foreground: {} ({})",
            "ℹ".blue().bold(),
            self.package.cyan(),
            self.process.yellow()
        );
        if let Some(pid) = self.pid {
            println!("  PID: {}", pid.to_string().yellow());
        }
        if let Some(activity) = self.activity.as_deref() {
            println!("  Activity: {}", activity.cyan());
        }
    }
}

pub async fn resolve_foreground_context(device_id: Option<&str>) -> Result<ForegroundContext> {
    let global_config = GlobalConfigManager::new()?.load().await?;
    let adb = AdbClient::new(Some(global_config.android.adb_path));
    let device = adb.get_device(device_id).await?;
    let foreground = adb.get_foreground_app(&device.id).await?;

    Ok(ForegroundContext {
        device,
        package: foreground.package,
        process: foreground.process,
        pid: foreground.pid,
        activity: foreground.activity,
    })
}

pub fn ensure_no_forbidden_args(
    raw_args: &[String],
    forbidden_args: &[&str],
    context: &str,
) -> Result<()> {
    let offending = raw_args.iter().find(|arg| {
        let arg = arg.as_str();
        forbidden_args
            .iter()
            .any(|forbidden| arg == *forbidden || arg.starts_with(&format!("{}=", forbidden)))
    });

    if let Some(arg) = offending {
        return Err(FridaMgrError::Config(format!(
            "{context}; do not pass '{arg}'. Use the pass-through command for full control."
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_non_forbidden_args() {
        let args = vec![
            "--no-pause".to_string(),
            "-l".to_string(),
            "a.js".to_string(),
        ];
        ensure_no_forbidden_args(&args, &["-D", "--device"], "ctx").unwrap();
    }

    #[test]
    fn blocks_exact_forbidden_arg() {
        let args = vec!["-D".to_string(), "emulator-5554".to_string()];
        let err = ensure_no_forbidden_args(&args, &["-D", "--device"], "ctx").unwrap_err();
        match err {
            FridaMgrError::Config(msg) => assert!(msg.contains("'-D'")),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn blocks_equals_style_forbidden_arg() {
        let args = vec!["--device=emulator-5554".to_string()];
        let err = ensure_no_forbidden_args(&args, &["--device"], "ctx").unwrap_err();
        match err {
            FridaMgrError::Config(msg) => assert!(msg.contains("'--device=emulator-5554'")),
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
