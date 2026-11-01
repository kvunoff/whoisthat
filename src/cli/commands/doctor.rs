use super::super::args::{DoctorArgs, GlobalArgs};
use super::super::error::CliError;
use super::super::output::Out;

pub async fn run(g: &GlobalArgs, args: DoctorArgs) -> Result<(), CliError> {
    let out = Out::new(g);
    let report = crate::doctor::run_doctor(args.logs).await;
    if g.json {
        out.json(&report);
    } else {
        report.print_pretty();
    }
    if !report.summary.is_healthy {
        return Err(CliError::Core("diagnostics found failures".into()));
    }
    Ok(())
}
