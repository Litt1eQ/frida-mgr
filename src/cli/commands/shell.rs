use crate::core::error::Result;
use crate::python::VenvExecutor;
use std::env;

pub async fn execute() -> Result<()> {
    let current_dir = env::current_dir()?;
    let executor = VenvExecutor::new(current_dir);

    let exit_code = executor.spawn_shell().await?;

    std::process::exit(exit_code);
}
