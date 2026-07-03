use crate::args::{ConfigArgs, ConfigCommand};

mod init;

pub fn run(args: ConfigArgs) -> anyhow::Result<()> {
    match args.command {
        ConfigCommand::Init { scope, force } => init::init(scope, args.path.as_deref(), force),
    }
}
