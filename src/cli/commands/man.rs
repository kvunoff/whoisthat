use clap::CommandFactory;

use crate::cli::{Cli, CliError};

pub fn run() -> Result<(), CliError> {
    clap_mangen::Man::new(Cli::command()).render(&mut std::io::stdout())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn man_page_starts_with_th() {
        let mut out = Vec::new();
        clap_mangen::Man::new(Cli::command())
            .render(&mut out)
            .expect("render man page");
        assert!(String::from_utf8_lossy(&out).contains(".TH"));
    }
}
