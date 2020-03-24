/*!
The `asuran-cli` binary provides a lightweight wrapper over the core `asuran`
logic, providing simple set of commands for directly interacting with
repositories.
*/
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
    // Our task in main is dead simple, we only need to parse the options and
    // match on the subcommand
    let options = Opt::from_args();
    let command = options.command.clone();
    match command {
        Command::New { .. } => new::new(options).await,
        Command::Store { target, name, .. } => store::store(options, target, name).await,
        Command::List { .. } => list::list(options).await,
        Command::Extract {
            target, archive, ..
        } => extract::extract(options, target, archive).await,
    }
}
