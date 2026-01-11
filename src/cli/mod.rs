pub mod commands;

use crate::config::AgentBuildTool;
use clap::{Parser, Subcommand};

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum InitServerSource {
    Download,
    Local,
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum AgentTool {
    FridaCompile,
    Esbuild,
}

impl From<AgentTool> for AgentBuildTool {
    fn from(value: AgentTool) -> Self {
        match value {
            AgentTool::FridaCompile => AgentBuildTool::FridaCompile,
            AgentTool::Esbuild => AgentBuildTool::Esbuild,
        }
    }
}

#[derive(Parser)]
#[command(
    name = "frida-mgr",
    version,
    about = "A comprehensive Frida version manager with Python environment management",
    long_about = None
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum AgentCommands {
    /// Create an agent TypeScript project scaffold
    Init {
        /// Agent directory (default: from frida.toml agent.dir, or "agent")
        #[arg(long)]
        dir: Option<String>,

        /// Build tool to use (default: from frida.toml agent.tool)
        #[arg(long, value_enum)]
        tool: Option<AgentTool>,

        /// Overwrite existing files
        #[arg(long)]
        force: bool,
    },

    /// Build the agent bundle
    Build {
        /// Agent directory (default: from frida.toml agent.dir, or "agent")
        #[arg(long)]
        dir: Option<String>,

        /// Build tool to use (default: from frida.toml agent.tool)
        #[arg(long, value_enum)]
        tool: Option<AgentTool>,
    },
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize a new Frida project
    Init {
        /// Frida version to install (default: latest)
        #[arg(short, long)]
        frida: Option<String>,

        /// Python version to use (default: 3.11)
        #[arg(short, long)]
        python: Option<String>,

        /// Android architecture (arm, arm64, x86, x86_64, auto)
        #[arg(short, long)]
        arch: Option<String>,

        /// Project name (default: current directory name)
        #[arg(short, long)]
        name: Option<String>,

        /// frida-server source: download from GitHub, or use a local binary
        #[arg(long, value_enum, default_value_t = InitServerSource::Download)]
        server_source: InitServerSource,

        /// Local frida-server binary path (required when --server-source=local)
        #[arg(long, required_if_eq("server_source", "local"))]
        local_server_path: Option<String>,

        /// frida-tools version to install (required when --server-source=local)
        #[arg(long, required_if_eq("server_source", "local"))]
        frida_tools: Option<String>,

        /// objection version to install (default: mapped by frida version, or let uv resolve)
        #[arg(long)]
        objection: Option<String>,
    },

    /// Install and switch to a specific Frida version
    Install {
        /// Frida version to install (e.g., 16.6.6, latest, stable)
        version: String,
    },

    /// List available or installed Frida versions
    List {
        /// Show only installed versions
        #[arg(short, long)]
        installed: bool,
    },

    /// Push frida-server to connected device
    Push {
        /// Device ID (default: first connected device)
        #[arg(short, long)]
        device: Option<String>,

        /// Automatically start the server after pushing
        #[arg(short, long)]
        start: bool,
    },

    /// Start frida-server on device
    Start {
        /// Device ID (default: first connected device)
        #[arg(short, long)]
        device: Option<String>,
    },

    /// Stop frida-server on device
    Stop {
        /// Device ID (default: first connected device)
        #[arg(short, long)]
        device: Option<String>,
    },

    /// Show device and server status
    Status {
        /// Device ID (default: first connected device)
        #[arg(short, long)]
        device: Option<String>,
    },

    /// List connected Android devices
    Devices,

    /// Check environment and dependencies
    Doctor,

    /// Run a command in the virtual environment
    Run {
        /// Command to run
        command: String,

        /// Arguments to pass to the command
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Run frida with the project's virtual environment (shortcut for 'run frida')
    #[command(name = "frida")]
    Frida {
        /// Arguments to pass to frida (e.g., -l script.js -U com.example.app)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Run objection with the project's virtual environment
    #[command(name = "objection")]
    Objection {
        /// Arguments to pass to objection (e.g., -g com.example.app explore)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Attach to the current foreground app and run frida (auto-detect process name)
    #[command(name = "top", visible_alias = "fg")]
    Top {
        /// Device ID (default: first connected device)
        #[arg(short, long)]
        device: Option<String>,

        /// Build a project agent and load it (-l); pass a directory or omit value for default "agent"
        #[arg(long, num_args = 0..=1, default_missing_value = "agent", value_name = "DIR")]
        agent: Option<String>,

        /// Agent build tool override (default: from frida.toml agent.tool)
        #[arg(long, value_enum)]
        agent_tool: Option<AgentTool>,

        /// JavaScript script to load (-l); can be repeated
        #[arg(short = 'l', long = "load")]
        scripts: Vec<String>,

        /// Extra frida arguments (excluding device/target selection)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Spawn the current foreground app and run frida
    #[command(name = "spawn", visible_alias = "sp")]
    Spawn {
        /// Device ID (default: first connected device)
        #[arg(short, long)]
        device: Option<String>,

        /// Build a project agent and load it (-l); pass a directory or omit value for default "agent"
        #[arg(long, num_args = 0..=1, default_missing_value = "agent", value_name = "DIR")]
        agent: Option<String>,

        /// Agent build tool override (default: from frida.toml agent.tool)
        #[arg(long, value_enum)]
        agent_tool: Option<AgentTool>,

        /// JavaScript script to load (-l); can be repeated
        #[arg(short = 'l', long = "load")]
        scripts: Vec<String>,

        /// Extra frida arguments (excluding device/target selection)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Run objection for the current foreground app (defaults to `explore`)
    #[command(name = "objection-fg", visible_alias = "og")]
    ObjectionFg {
        /// Device ID (default: first connected device)
        #[arg(short, long)]
        device: Option<String>,

        /// Objection arguments after the auto-injected target selector (e.g., `--name <package>`)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Run frida-ps with the project's virtual environment
    #[command(name = "ps")]
    Ps {
        /// Arguments to pass to frida-ps
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Run frida-trace with the project's virtual environment
    #[command(name = "trace")]
    Trace {
        /// Arguments to pass to frida-trace
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Enter the virtual environment shell
    Shell,

    /// Run uv (pass-through)
    #[command(name = "uv")]
    Uv {
        /// Arguments to pass to uv
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Run uv pip with the project's virtual environment selected
    #[command(name = "pip")]
    Pip {
        /// Arguments to pass to uv pip
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Sync project environment with frida.toml (and optionally refresh version mapping)
    Sync {
        /// Update version mapping file from GitHub releases
        #[arg(long)]
        update_map: bool,

        /// Include prerelease versions when updating the mapping
        #[arg(long, requires = "update_map")]
        prerelease: bool,

        /// Only update the mapping; do not sync the current project
        #[arg(long)]
        no_project: bool,

        /// Recreate the virtual environment (required when python.version changes)
        #[arg(long)]
        recreate_venv: bool,
    },

    /// Manage TypeScript agent scaffold/build
    Agent {
        #[command(subcommand)]
        command: AgentCommands,
    },
}

pub async fn run(cli: Cli) -> crate::core::error::Result<()> {
    match cli.command {
        Commands::Init {
            frida,
            python,
            arch,
            name,
            server_source,
            local_server_path,
            frida_tools,
            objection,
        } => {
            commands::init::execute(
                frida,
                python,
                arch,
                name,
                server_source,
                local_server_path,
                frida_tools,
                objection,
            )
            .await
        }

        Commands::Install { version } => commands::install::execute(version).await,

        Commands::List { installed } => commands::list::execute(installed).await,

        Commands::Push { device, start } => commands::push::execute(device, start).await,

        Commands::Start { device } => commands::start::execute(device).await,

        Commands::Stop { device } => commands::stop::execute(device).await,

        Commands::Status { device } => commands::status::execute(device).await,

        Commands::Devices => commands::devices::execute().await,

        Commands::Doctor => commands::doctor::execute().await,

        Commands::Run { command, args } => commands::run::execute(command, args).await,

        Commands::Frida { args } => commands::frida::execute(args).await,

        Commands::Objection { args } => commands::objection::execute(args).await,

        Commands::Top {
            device,
            agent,
            agent_tool,
            scripts,
            args,
        } => {
            commands::top::execute(device, agent, agent_tool.map(Into::into), scripts, args).await
        }

        Commands::Spawn {
            device,
            agent,
            agent_tool,
            scripts,
            args,
        } => commands::spawn::execute(device, agent, agent_tool.map(Into::into), scripts, args)
            .await,

        Commands::ObjectionFg { device, args } => {
            commands::objection_fg::execute(device, args).await
        }

        Commands::Ps { args } => commands::run::execute("frida-ps".to_string(), args).await,

        Commands::Trace { args } => commands::run::execute("frida-trace".to_string(), args).await,

        Commands::Shell => commands::shell::execute().await,

        Commands::Uv { args } => commands::uv::execute(args).await,

        Commands::Pip { args } => commands::pip::execute(args).await,

        Commands::Sync {
            update_map,
            prerelease,
            no_project,
            recreate_venv,
        } => commands::sync::execute(update_map, prerelease, no_project, recreate_venv).await,

        Commands::Agent { command } => match command {
            AgentCommands::Init { dir, tool, force } => {
                commands::agent::init(dir, tool.map(Into::into), force).await
            }
            AgentCommands::Build { dir, tool } => {
                commands::agent::build(dir, tool.map(Into::into)).await
            }
        },
    }
}
