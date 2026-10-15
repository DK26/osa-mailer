use anyhow::Context;
use std::env;

fn main() -> anyhow::Result<()> {
    let current_exe =
        env::current_exe().context("Unable to get the current binary file from the OS.")?;
    let current_exe_dir = current_exe
        .parent()
        .context("Unable to get current binary file directory")?;

    let config = osa_mailer::RunConfig::from_env(current_exe_dir)?;

    if matches!(
        config.auth,
        osa_mailer::send::Authentication::NoAuth
    ) && config.credentials.is_some()
    {
        eprintln!("Warning: USERNAME/PASSWORD are set but AUTH=noauth; the credentials will be ignored.");
    }

    println!(
        "Mail-Relay: \"{}:{}\" [{}]",
        config.server, config.port, config.auth
    );

    osa_mailer::run(config)
}
