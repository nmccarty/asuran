/*!
The `asuran-cli` binary provides a lightweight wrapper over the core `asuran`
logic, providing simple set of commands for directly interacting with
repositories.
 */
#[cfg_attr(tarpaulin, skip)]
mod cli;
#[cfg_attr(tarpaulin, skip)]
mod util;

#[cfg_attr(tarpaulin, skip)]
mod bench;
#[cfg_attr(tarpaulin, skip)]
mod contents;
#[cfg_attr(tarpaulin, skip)]
mod extract;
#[cfg_attr(tarpaulin, skip)]
mod list;
#[cfg_attr(tarpaulin, skip)]
mod new;
#[cfg_attr(tarpaulin, skip)]
mod store;

use anyhow::Result;
use cli::{Command, Opt};
use structopt::StructOpt;

#[cfg_attr(tarpaulin, skip)]
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
        Command::BenchCrypto => bench::bench_crypto().await,
        Command::Contents {
            archive, glob_opts, ..
        } => contents::contents(options, archive, glob_opts).await,
    }
}
