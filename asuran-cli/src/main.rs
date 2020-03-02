mod cli;
mod util;

mod extract;
mod list;
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
        Command::New => new::new(options).await,
        Command::Store { target, name } => store::store(options, target, name).await,
        Command::List => list::list(options).await,
        Command::Extract { target, archive } => extract::extract(options, target, archive).await,
    }
}
