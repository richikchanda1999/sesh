use clap::CommandFactory;
use clap_complete::generate;

use crate::cli::Cli;

pub fn run(shell: clap_complete::Shell) {
    let mut cmd = Cli::command();
    generate(shell, &mut cmd, "sesh", &mut std::io::stdout());
}
