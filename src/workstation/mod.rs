use clap::ArgMatches;

pub mod check;
pub mod setup;

pub fn execute(args: &ArgMatches) -> anyhow::Result<()> {
    match args.subcommand() {
        None => {
            let result = inquire::Select::new("Workstation Command:", vec!["Check", "Setup"])
                .prompt();
            match result {
                Ok("Check") => {
                    return check::execute_interactive(args);
                }
                Ok("Setup") => {
                    return setup::execute(args);
                }
                Ok(_) => {
                    unreachable!("Prevented by Inquire")
                }
                Err(_) => {}
            }
        }
        Some(("check", sub_args)) => {
            return check::execute(sub_args);
        }
        Some(("setup", sub_args)) => {
            return setup::execute(sub_args);
        }
        Some((_, _)) => {
            unreachable!("Prevented by Clap")
        }
    }

    Ok(())
}