use crate::cli::commands::foreground::{ensure_no_forbidden_args, resolve_foreground_context};
use crate::cli::commands::script::resolve_existing_script_path;
use crate::config::{AgentBuildTool, ProjectConfigManager};
use crate::core::error::Result;
use crate::{agent, agent::AgentProject};
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
    agent_dir: Option<String>,
    agent_tool: Option<AgentBuildTool>,
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

    let current_dir = env::current_dir()?;
    let project_dir =
        ProjectConfigManager::find_project_root(&current_dir).unwrap_or_else(|| current_dir.clone());

    let mut frida_args = Vec::with_capacity(8 + scripts.len() * 2 + args.len());
    frida_args.push("-D".to_string());
    frida_args.push(foreground.device.id);
    frida_args.push("-f".to_string());
    frida_args.push(foreground.package);

    if let Some(dir) = agent_dir.as_deref() {
        let project_mgr = ProjectConfigManager::new(&project_dir);
        let mut config = project_mgr.load().await?;
        config.agent.dir = dir.to_string();
        if let Some(tool) = agent_tool {
            config.agent.tool = tool;
        }
        let agent_project = AgentProject::from_agent_config(project_dir.clone(), &config.agent);
        let out = agent::build_agent(&agent_project).await?;
        frida_args.push("-l".to_string());
        frida_args.push(out.to_string_lossy().to_string());
    }

    for script in scripts {
        frida_args.push("-l".to_string());
        frida_args.push(resolve_existing_script_path(&current_dir, &project_dir, &script));
    }

    frida_args.extend(args);

    let executor = VenvExecutor::new(project_dir);
    let exit_code = executor.run_interactive("frida", &frida_args).await?;

    std::process::exit(exit_code);
}
