mod cli;
mod util;

mod new;

use anyhow::Result;
use cli::{Command, Opt};
use structopt::StructOpt;

#[tokio::main]
async fn main() -> Result<()> {
    let options = Opt::from_args();
    let command = options.command.clone();
    match command {
        Command::New { .. } => new::new(options).await,
        _ => todo!(),
    }
}
