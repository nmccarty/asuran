mod cli;

use cli::{Command, Opt};
use structopt::StructOpt;

#[tokio::main]
async fn main() {
    let options = Opt::from_args();
}
