use crate::agent::{build_agent, scaffold_agent_project, AgentProject};
use crate::config::{AgentBuildTool, ProjectConfigManager};
use crate::core::error::Result;
use colored::Colorize;
use std::env;

pub async fn init(
    dir: Option<String>,
    tool: Option<AgentBuildTool>,
    force: bool,
) -> Result<()> {
    let project_dir = resolve_project_dir()?;
    let project_mgr = ProjectConfigManager::new(&project_dir);
    let mut config = project_mgr.load().await?;

    if let Some(dir) = dir {
        config.agent.dir = dir;
    }
    if let Some(tool) = tool {
        config.agent.tool = tool;
    }

    let agent = AgentProject::from_agent_config(project_dir, &config.agent);
    scaffold_agent_project(&agent, &config.agent, &config.project.name, force).await?;
    Ok(())
}

pub async fn build(dir: Option<String>, tool: Option<AgentBuildTool>) -> Result<()> {
    let project_dir = resolve_project_dir()?;
    let project_mgr = ProjectConfigManager::new(&project_dir);
    let mut config = project_mgr.load().await?;

    if let Some(dir) = dir {
        config.agent.dir = dir;
    }
    if let Some(tool) = tool {
        config.agent.tool = tool;
    }

    let agent = AgentProject::from_agent_config(project_dir, &config.agent);
    let out = build_agent(&agent).await?;

    println!("  Use with: {}", format!("frida -l {}", out.display()).cyan());
    Ok(())
}

fn resolve_project_dir() -> Result<std::path::PathBuf> {
    let cwd = env::current_dir()?;
    Ok(ProjectConfigManager::find_project_root(&cwd).unwrap_or_else(|| {
        eprintln!(
            "{} No frida.toml found in parents; using current directory: {}",
            "⚠".yellow().bold(),
            cwd.display().to_string().yellow()
        );
        cwd
    }))
}
