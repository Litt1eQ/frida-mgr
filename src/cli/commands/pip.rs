use crate::core::error::Result;
use crate::python::UvManager;
use std::env;

pub async fn execute(args: Vec<String>) -> Result<()> {
    let current_dir = env::current_dir()?;
    let uv_mgr = UvManager::new(current_dir);

    let exit_code = uv_mgr.run_uv_pip_interactive(&args).await?;
    std::process::exit(exit_code);
}

