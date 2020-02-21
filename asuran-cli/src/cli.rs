use clap::{arg_enum, AppSettings};
use std::path::PathBuf;
use structopt::StructOpt;

const VERSION: &'static str = concat!(
    env!("VERGEN_SEMVER"),
    "-",
    env!("VERGEN_SHA_SHORT"),
    " ",
    env!("VERGEN_BUILD_DATE"),
);

#[derive(Debug, StructOpt)]
#[structopt(
    name = "Asuran-CLI",
    about = "Deduplicating, encrypting, tamper evident archiver",
    author = env!("CARGO_PKG_AUTHORS"),
    version = VERSION,
    global_setting(AppSettings::ColoredHelp),
)]
pub struct Opt {
    /// Location of the Asuran repository
    #[structopt(name = "REPO")]
    repo: PathBuf,
    /// Password for the repository. Can also be specified with the PASSWORD enviroment variable
    #[structopt(short, long, env = "ASURAN_PASSWORD", hide_env_values = true)]
    password: String,
    /// Type of repository to use
    #[structopt(
        short,
        long,
        default_value = "MultiFile",
        case_insensitive(true),
        possible_values(&RepositoryType::variants())
    )]
    repository_type: RepositoryType,
    /// Selects Encryption Algorithm
    #[structopt(
        short,
        long,
        default_value = "AES256CTR",
        case_insensitive(true),
        possible_values(&Encryption::variants())
    )]
    encryption: Encryption,
    /// Selects Compression Algorithm
    #[structopt(
        short,
        long,
        default_value = "ZStd",
        case_insensitive(true),
        possible_values(&Encryption::variants())
    )]
    compression: Compression,
    /// Sets compression level. Defaults to the compression algorithim's "middle" setting
    #[structopt(short = "l", long)]
    compression_level: Option<u32>,
    /// Sets the HMAC algorthim used. Note: this will not change the HMAC algorthim used on an
    /// existing repository
    #[structopt(
        short,
        long,
        default_value = "Blake3",
        case_insensitive(true),
        possible_values(&HMAC::variants())
    )]
    hmac: HMAC,
    /// Operation to perform
    #[structopt(subcommand)]
    command: Command,
}

arg_enum! {
    #[derive(Debug)]
    pub enum RepositoryType {
        MultiFile,
        FlatFile,
    }
}

arg_enum! {
    #[derive(Debug)]
    pub enum Encryption {
        AES256CBC,
        AES256CTR,
        ChaCha20,
        None,
    }
}
arg_enum! {
   #[derive(Debug)]
   pub enum Compression {
       ZStd,
       LZ4,
       LZMA,
       None
   }
}

arg_enum! {
    #[derive(Debug)]
    pub enum HMAC {
        SHA256,
        Blake2b,
        Blake2bp,
        Blake3,
        SHA3,
    }
}

#[derive(StructOpt, Debug)]
pub enum Command {
    /// Provides a listing of the archives in a repository
    List,
    /// Creates a new archive in a repository
    Store {
        /// Location of the directory to store
        #[structopt(name = "TARGET")]
        target: PathBuf,
        /// Name for the new archive. Defaults to an ISO date/time stamp
        #[structopt(short, long)]
        name: Option<String>,
    },
    /// Extracts an archive from a repository
    Extract {
        /// Location to restore to
        #[structopt(name = "TARGET")]
        target: PathBuf,
        /// Name or ID of the archive to be restored
        #[structopt(name = "ARCHIVE")]
        archive: String,
    },
    /// Creates a new repository
    New,
}
