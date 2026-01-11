use crate::config::schema::{AgentBuildTool, AgentConfig, ProjectConfig};
use crate::core::error::{FridaMgrError, Result};
use crate::core::{ensure_dir_exists, resolve_path};
use colored::Colorize;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::fs;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct AgentProject {
    pub project_dir: PathBuf,
    pub agent_dir: PathBuf,
    pub entry_path: PathBuf,
    pub out_path: PathBuf,
    pub tool: AgentBuildTool,
}

impl AgentProject {
    pub fn from_config(project_dir: PathBuf, config: &ProjectConfig) -> Self {
        let agent_dir = resolve_path(&project_dir, &config.agent.dir);
        let entry_path = resolve_path(&agent_dir, &config.agent.entry);
        let out_path = resolve_path(&agent_dir, &config.agent.out);
        Self {
            project_dir,
            agent_dir,
            entry_path,
            out_path,
            tool: config.agent.tool.clone(),
        }
    }

    pub fn from_agent_config(project_dir: PathBuf, config: &AgentConfig) -> Self {
        let agent_dir = resolve_path(&project_dir, &config.dir);
        let entry_path = resolve_path(&agent_dir, &config.entry);
        let out_path = resolve_path(&agent_dir, &config.out);
        Self {
            project_dir,
            agent_dir,
            entry_path,
            out_path,
            tool: config.tool.clone(),
        }
    }

    pub fn with_tool(mut self, tool: AgentBuildTool) -> Self {
        self.tool = tool;
        self
    }
}

pub async fn scaffold_agent_project(
    agent: &AgentProject,
    config: &AgentConfig,
    project_name: &str,
    force: bool,
) -> Result<()> {
    let agent_dir = &agent.agent_dir;

    if agent_dir.exists() && !force {
        let mut entries = fs::read_dir(agent_dir).await?;
        if entries.next_entry().await?.is_some() {
            return Err(FridaMgrError::Config(format!(
                "Agent directory already exists and is not empty: {} (use --force to overwrite)",
                agent_dir.display()
            )));
        }
    }

    ensure_dir_exists(agent_dir).await?;
    if let Some(parent) = agent.entry_path.parent() {
        ensure_dir_exists(parent).await?;
    }
    if let Some(parent) = agent.out_path.parent() {
        ensure_dir_exists(parent).await?;
    }

    write_file(
        &agent_dir.join(".gitignore"),
        template_gitignore(),
        force,
    )
    .await?;

    write_file(
        &agent_dir.join("tsconfig.json"),
        template_tsconfig_json(),
        force,
    )
    .await?;

    write_file(
        &agent_dir.join("package.json"),
        template_package_json(project_name, config),
        force,
    )
    .await?;

    write_file(
        &agent.entry_path,
        template_index_ts(config),
        force,
    )
    .await?;

    write_file(
        &agent
            .entry_path
            .parent()
            .unwrap_or(&agent.agent_dir)
            .join("env.d.ts"),
        template_env_d_ts(),
        force,
    )
    .await?;

    write_file(
        &agent_dir.join("README.md"),
        template_agent_readme(config),
        force,
    )
    .await?;

    println!(
        "{} Agent scaffold created at {}",
        "✓".green().bold(),
        agent_dir.display().to_string().yellow()
    );
    println!(
        "  Next: {} (inside agent dir), then {}",
        "npm install".cyan(),
        "frida-mgr agent build".cyan()
    );

    Ok(())
}

pub async fn build_agent(agent: &AgentProject) -> Result<PathBuf> {
    if !agent.entry_path.is_file() {
        return Err(FridaMgrError::FileNotFound(format!(
            "Agent entry not found: {}",
            agent.entry_path.display()
        )));
    }

    let env_d_ts = agent
        .entry_path
        .parent()
        .unwrap_or(&agent.agent_dir)
        .join("env.d.ts");
    if agent.entry_path.extension().and_then(|e| e.to_str()) == Some("ts") && !env_d_ts.is_file() {
        return Err(FridaMgrError::Config(format!(
            "Missing {} next to the agent entry (required for console typings). Run {} to regenerate the scaffold.",
            env_d_ts.display(),
            "frida-mgr agent init --force".cyan()
        )));
    }

    let out_parent = agent
        .out_path
        .parent()
        .ok_or_else(|| FridaMgrError::Config("Invalid agent.out path".to_string()))?;
    ensure_dir_exists(out_parent).await?;

    let (bin_name, args) = match agent.tool {
        AgentBuildTool::FridaCompile => (
            "frida-compile",
            vec![
                agent.entry_path.to_string_lossy().to_string(),
                "-o".to_string(),
                agent.out_path.to_string_lossy().to_string(),
            ],
        ),
        AgentBuildTool::Esbuild => (
            "esbuild",
            vec![
                agent.entry_path.to_string_lossy().to_string(),
                "--bundle".to_string(),
                "--platform=neutral".to_string(),
                "--format=iife".to_string(),
                "--target=es2020".to_string(),
                format!("--outfile={}", agent.out_path.to_string_lossy()),
            ],
        ),
    };

    let bin_path = local_node_bin(&agent.agent_dir, bin_name);
    if !bin_path.exists() {
        return Err(FridaMgrError::Config(format!(
            "Missing {} in {}. Run {} in the agent directory first.",
            bin_name.cyan(),
            "node_modules/.bin".yellow(),
            "npm install".cyan()
        )));
    }

    println!(
        "{} Building agent with {}...",
        "⚙".blue().bold(),
        agent.tool.as_str().cyan()
    );

    let status = Command::new(&bin_path)
        .args(&args)
        .current_dir(&agent.agent_dir)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await
        .map_err(|e| FridaMgrError::CommandFailed(format!("Failed to run {}: {}", bin_name, e)))?;

    if !status.success() {
        return Err(FridaMgrError::CommandFailed(format!(
            "{} failed with exit code {:?}",
            bin_name,
            status.code()
        )));
    }

    println!(
        "{} Built agent: {}",
        "✓".green().bold(),
        agent.out_path.display().to_string().yellow()
    );

    Ok(agent.out_path.clone())
}

fn local_node_bin(agent_dir: &Path, name: &str) -> PathBuf {
    let bin = if cfg!(windows) {
        format!("{}.cmd", name)
    } else {
        name.to_string()
    };
    agent_dir.join("node_modules").join(".bin").join(bin)
}

async fn write_file(path: &Path, content: String, force: bool) -> Result<()> {
    if path.exists() && !force {
        return Err(FridaMgrError::Config(format!(
            "Refusing to overwrite existing file: {} (use --force)",
            path.display()
        )));
    }
    if let Some(parent) = path.parent() {
        ensure_dir_exists(parent).await?;
    }
    fs::write(path, content).await?;
    Ok(())
}

fn template_gitignore() -> String {
    r#"/dist
/node_modules
*.log
"#
    .to_string()
}

fn template_tsconfig_json() -> String {
    r#"{
  "compilerOptions": {
    "target": "ES2020",
    "lib": ["ES2020"],
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "strict": true,
    "skipLibCheck": true,
    "types": ["frida-gum"]
  },
  "include": ["src/**/*.ts", "src/**/*.d.ts"]
}
"#
    .to_string()
}

fn template_package_json(project_name: &str, config: &AgentConfig) -> String {
    let entry = config.entry.as_str();
    let out = config.out.as_str();
    let (build, watch, dev_deps) = match config.tool {
        AgentBuildTool::FridaCompile => (
            format!("frida-compile {entry} -o {out}"),
            format!("frida-compile {entry} -o {out} -w"),
            r#""frida-compile": "latest",
    "@types/frida-gum": "latest",
    "typescript": "latest""#,
        ),
        AgentBuildTool::Esbuild => (
            format!("esbuild {entry} --bundle --platform=neutral --format=iife --target=es2020 --outfile={out}"),
            format!("esbuild {entry} --bundle --platform=neutral --format=iife --target=es2020 --outfile={out} --watch"),
            r#""esbuild": "latest",
    "@types/frida-gum": "latest",
    "typescript": "latest""#,
        ),
    };

    let safe_name = project_name
        .trim()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    let pkg_name = if safe_name.is_empty() {
        "frida-agent".to_string()
    } else {
        format!("{}-agent", safe_name)
    };

    format!(
        r#"{{
  "name": "{pkg_name}",
  "private": true,
  "version": "0.0.0",
  "scripts": {{
    "build": "{build}",
    "watch": "{watch}"
  }},
  "devDependencies": {{
    {dev_deps}
  }}
}}
"#
    )
}

fn template_index_ts(config: &AgentConfig) -> String {
    let entry_hint = config.entry.clone();
    format!(
        r#"/// <reference path="./env.d.ts" />
// Entry: {entry_hint}
console.log("[frida-mgr] agent loaded");

// A tiny Android-friendly example. Safe to run even if Java isn't available.
const JavaApi = (globalThis as any).Java;
if (JavaApi && JavaApi.available) {{
  JavaApi.perform(() => {{
    console.log("[frida-mgr] Java is available");
  }});
}} else {{
  console.log("[frida-mgr] Java not available");
}}
"#
    )
}

fn template_env_d_ts() -> String {
    r#"declare const console: {
  log: (...args: any[]) => void;
  warn: (...args: any[]) => void;
  error: (...args: any[]) => void;
};
"#
    .to_string()
}

fn template_agent_readme(config: &AgentConfig) -> String {
    format!(
        r#"# Frida Agent

This folder is generated by `frida-mgr agent init`.

## Setup

From the project root:

```bash
cd {dir}
npm install
```

## Build

```bash
npm run build
```

Outputs to `{out}`.

## Notes

- This project intentionally does not include `DOM`/`@types/node` typings to avoid misleading APIs in Frida.
- `console` is declared in `src/env.d.ts`.

## Use with frida-mgr

```bash
frida-mgr top --agent {dir}
frida-mgr spawn --agent {dir} -- --no-pause
```
"#,
        dir = config.dir,
        out = config.out
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_json_contains_selected_tool() {
        let cfg = AgentConfig {
            tool: AgentBuildTool::FridaCompile,
            ..AgentConfig::default()
        };
        let p = template_package_json("demo", &cfg);
        assert!(p.contains("\"frida-compile\""));
        let cfg = AgentConfig {
            tool: AgentBuildTool::Esbuild,
            ..AgentConfig::default()
        };
        let p = template_package_json("demo", &cfg);
        assert!(p.contains("\"esbuild\""));
    }

    #[test]
    fn default_config_paths_are_relative() {
        let cfg = AgentConfig::default();
        assert!(!cfg.dir.is_empty());
        assert!(!cfg.entry.is_empty());
        assert!(!cfg.out.is_empty());
    }
}
