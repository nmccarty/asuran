mod cli;
mod util;

mod new;
mod store;

use anyhow::Result;
use cli::{Command, Opt};
use structopt::StructOpt;

#[tokio::main]
async fn main() -> Result<()> {
    let options = Opt::from_args();
    let command = options.command.clone();
    match command {
        Command::New { .. } => new::new(options).await,
        Command::Store { target, name } => store::store(options, target, name).await,
        _ => todo!(),
    }
}
