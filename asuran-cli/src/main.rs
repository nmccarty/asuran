mod cli;
mod util;

use cli::{Command, Opt};
use structopt::StructOpt;

#[tokio::main]
async fn main() {
    let options: &'static Opt = Box::leak(Box::new(Opt::from_args()));
}
