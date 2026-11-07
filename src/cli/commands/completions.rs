use clap::CommandFactory;

use crate::cli::{Cli, CliError};

pub fn run(shell: clap_complete::Shell) -> Result<(), CliError> {
    let mut cmd = Cli::command();
    clap_complete::generate(shell, &mut cmd, "whoisthat", &mut std::io::stdout());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bash_output_is_not_empty() {
        let mut cmd = Cli::command();
        let mut out = Vec::new();
        clap_complete::generate(clap_complete::Shell::Bash, &mut cmd, "whoisthat", &mut out);
        assert!(!out.is_empty());
        assert!(String::from_utf8_lossy(&out).contains("whoisthat"));
    }
}
