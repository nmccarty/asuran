/*!
The `cli` module provides the data types used for parsing the command line
arguements, as well as some utility functions for converting those types to
their equivlants in `asuran` proper.
*/
use asuran::repository::backend::object_wrappers::BackendObject;
use asuran::repository::{self, Backend, Key};

use anyhow::{anyhow, Context, Result};
use clap::{arg_enum, AppSettings};
use repository::backend::{flatfile, multifile};
use structopt::StructOpt;

use std::env;
use std::fs::metadata;
use std::path::PathBuf;

/// The version + git commit + build date string the program idenitifes itself
/// with
const VERSION: &str = concat!(
    env!("VERGEN_SEMVER"),
    "-",
    env!("VERGEN_SHA_SHORT"),
    " ",
    env!("VERGEN_BUILD_DATE"),
);

arg_enum! {
    /// Identifies which backend the user has selected.
    ///
    /// These are a 1-to-1 corrospondance with the name of the struct
    /// implementing that backend in the `asuran` crate.
    #[derive(Debug, Clone)]
    pub enum RepositoryType {
        MultiFile,
        FlatFile,
        SFTP,
    }
}

arg_enum! {
    /// The type of Encryption the user has selected
    ///
    /// These are, more or less, a 1-to-1 corrospondance with the name of the
    /// `Encryption` enum variant in the `asuran` crate, but these do not carry
    /// an IV with them.
    #[derive(Debug, Clone)]
    pub enum Encryption {
        AES256CTR,
        ChaCha20,
        None,
    }
}
arg_enum! {
   /// The type of compression the user has selected
   ///
   /// These are, more or less, a 1-to-1 corrospondance with the name of the
   /// `Compression` enum variant in the `asuran` crate, but these do not carry
   /// a compression level with them.
   #[derive(Debug, Clone)]
   pub enum Compression {
       ZStd,
       LZ4,
       LZMA,
       None
   }
}

arg_enum! {
    /// The HMAC algorithim the user has selected
    ///
    /// These are a 1-to-1 corrospondance with the `HMAC` enum variant in the
    /// `asuran` crate
    #[derive(Debug, Clone)]
    pub enum HMAC {
        SHA256,
        Blake2b,
        Blake2bp,
        Blake3,
        SHA3,
    }
}

/// A high performance, de-duplicating archiver, with no-compromises security.
#[derive(StructOpt, Debug, Clone)]
pub enum Command {
    /// Provides a listing of the archives in a repository
    List {
        #[structopt(flatten)]
        repo_opts: RepoOpt,
    },
    /// Creates a new archive in a repository
    Store {
        #[structopt(flatten)]
        repo_opts: RepoOpt,
        /// Location of the directory to store
        #[structopt(name = "TARGET")]
        target: PathBuf,
        /// Name for the new archive. Defaults to an ISO date/time stamp
        #[structopt(short, long)]
        name: Option<String>,
    },
    /// Extracts an archive from a repository
    Extract {
        #[structopt(flatten)]
        repo_opts: RepoOpt,
        #[structopt(flatten)]
        glob_opts: GlobOpt,
        /// Location to restore to
        #[structopt(name = "TARGET")]
        target: PathBuf,
        /// Name or ID of the archive to be restored
        #[structopt(name = "ARCHIVE")]
        archive: String,
        /// Preview an extraction without actually performing it
        ///
        /// More or less equivalent to contents, but with the same syntax as a normal
        /// restore command.
        #[structopt(short = "P", long)]
        preview: bool,
    },
    /// Creates a new repository
    New {
        #[structopt(flatten)]
        repo_opts: RepoOpt,
    },
    /// Runs benchmarks on all combinations of asuran's supported crypto primitives.
    BenchCrypto,
    /// Lists the contents of an archive, with optional glob filters
    Contents {
        #[structopt(flatten)]
        repo_opts: RepoOpt,
        #[structopt(flatten)]
        glob_opts: GlobOpt,
        /// Name or ID of the archive to list the contents of
        #[structopt(name = "ARCHIVE")]
        archive: String,
    },
}

impl Command {
    pub fn repo_opts(&self) -> &RepoOpt {
        match self {
            Self::List { repo_opts, .. } => repo_opts,
            Self::Store { repo_opts, .. } => repo_opts,
            Self::Extract { repo_opts, .. } => repo_opts,
            Self::New { repo_opts, .. } => repo_opts,
            Self::Contents {repo_opts, ..} => repo_opts,
            Self::BenchCrypto => unimplemented!("asuran-cli bench does not interact with a repository, and does not have repository options."),
        }
    }
}

/// Shared glob matching options
#[derive(Debug, StructOpt, Clone)]
pub struct GlobOpt {
    /// Patterns to include.
    ///
    /// Having this option present will result in only files matched by one of the
    /// provided globs being included in the operation, with the exception of any files
    /// matched by one of the exclude globs, if present.
    #[structopt(short = "I", long)]
    pub include: Option<Vec<String>>,
    /// Patterns to exclude
    ///
    /// Any files matching globs provided as an exclude will not be included in the
    /// operation.
    #[structopt(short = "E", long)]
    pub exclude: Option<Vec<String>>,
}

/// Options that are shared among all repository commands
#[derive(Debug, StructOpt, Clone)]
pub struct RepoOpt {
    /// Location of the Asuran repository
    #[structopt(name = "REPO")]
    pub repo: PathBuf,
    /// Password for the repository. Can also be specified with the PASSWORD
    /// enviroment variable
    #[structopt(short, long, env = "ASURAN_PASSWORD", hide_env_values = true)]
    pub password: String,
    /// Type of repository to use
    #[structopt(
        short,
        long,
        default_value = "MultiFile",
        case_insensitive(true),
        possible_values(&RepositoryType::variants())
    )]
    pub repository_type: RepositoryType,
    /// Selects Encryption Algorithm
    #[structopt(
        short,
        long,
        default_value = "AES256CTR",
        case_insensitive(true),
        possible_values(&Encryption::variants())
    )]
    pub encryption: Encryption,
    /// Selects Compression Algorithm
    #[structopt(
        short,
        long,
        default_value = "ZStd",
        case_insensitive(true),
        possible_values(&Compression::variants())
    )]
    pub compression: Compression,
    /// Sets compression level. Defaults to the compression algorithim's
    /// "middle" setting
    #[structopt(short = "l", long)]
    pub compression_level: Option<u32>,
    /// Sets the HMAC algorthim used. Note: this will not change the HMAC
    /// algorthim used on an existing repository
    #[structopt(
        short,
        long,
        default_value = "Blake3",
        case_insensitive(true),
        possible_values(&HMAC::variants())
    )]
    pub hmac: HMAC,
    /// Password to use for SFTP connection for SFTP backend.
    ///
    /// Will attempt to use ssh-agent authentication if not set.
    #[structopt(long, env = "ASURAN_SFTP_PASSWORD", hide_env_values = true)]
    pub sftp_password: Option<String>,
    /// Port to use for the SFTP connection to the SFTP backend.
    ///
    /// Will default to 22 if not specified
    #[structopt(long, env = "ASURAN_SFTP_PORT")]
    pub sftp_port: Option<u16>,
}

/// Struct for holding the options the user has selected
#[derive(Debug, StructOpt)]
#[structopt(
    name = "Asuran-CLI",
    about = "Deduplicating, encrypting, tamper evident archiver",
    author = env!("CARGO_PKG_AUTHORS"),
    version = VERSION,
    global_setting(AppSettings::ColoredHelp),
)]

/// A high performance, de-duplicating archiver, with no-compromises security.
pub struct Opt {
    /// Operation to perform
    #[structopt(subcommand)]
    pub command: Command,
    /// Squelch non-logging operations
    #[structopt(short, long, global = true)]
    pub quiet: bool,
    /// Number of tasks to spawn for the chunk processing pipeline.
    ///
    /// Defaults to 0, which corresponds to the number of CPUs on the system.
    #[structopt(short = "T", long, default_value = "0", global = true)]
    pub pipeline_tasks: usize,
}

impl Opt {
    pub fn get_chunk_settings(&self) -> repository::ChunkSettings {
        self.command.repo_opts().get_chunk_settings()
    }
    pub async fn open_repo_backend(&self) -> Result<(BackendObject, Key)> {
        self.command
            .repo_opts()
            .open_repo_backend(self.pipeline_tasks() * 8)
            .await
    }
    pub fn repo_opts(&self) -> &RepoOpt {
        self.command.repo_opts()
    }
    pub fn pipeline_tasks(&self) -> usize {
        if self.pipeline_tasks == 0 {
            num_cpus::get()
        } else {
            self.pipeline_tasks
        }
    }
}

impl RepoOpt {
    /// Generates an `asuran::repostiory::ChunkSettings` from the options the
    /// user has selected
    pub fn get_chunk_settings(&self) -> repository::ChunkSettings {
        let compression = match self.compression {
            Compression::ZStd => self
                .compression_level
                .map(|x| repository::Compression::ZStd { level: x as i32 })
                .unwrap_or(repository::Compression::ZStd { level: 3 }),
            Compression::LZ4 => self
                .compression_level
                .map(|x| repository::Compression::LZ4 { level: x })
                .unwrap_or(repository::Compression::LZ4 { level: 4 }),
            Compression::None => repository::Compression::NoCompression,
            Compression::LZMA => self
                .compression_level
                .map(|x| repository::Compression::LZMA { level: x })
                .unwrap_or(repository::Compression::LZMA { level: 6 }),
        };

        let encryption = match self.encryption {
            Encryption::AES256CTR => repository::Encryption::new_aes256ctr(),
            Encryption::ChaCha20 => repository::Encryption::new_chacha20(),
            Encryption::None => repository::Encryption::NoEncryption,
        };

        let hmac = match self.hmac {
            HMAC::SHA256 => repository::HMAC::SHA256,
            HMAC::Blake2b => repository::HMAC::Blake2b,
            HMAC::Blake2bp => repository::HMAC::Blake2bp,
            HMAC::Blake3 => repository::HMAC::Blake3,
            HMAC::SHA3 => repository::HMAC::SHA3,
        };

        repository::ChunkSettings {
            compression,
            encryption,
            hmac,
        }
    }

    /// Attempts to open up a connection to the repostiory, based on the information
    /// passed in the Options
    ///
    /// # Errors
    ///
    /// Will return Err if
    ///
    /// 1. The give repository path is of the wrong type (i.e a folder when a FlatFile
    ///    was requested)
    /// 2. Some other error defined in the repostiory implementation occurs trying to open it
    pub async fn open_repo_backend(&self, queue_depth: usize) -> Result<(BackendObject, Key)> {
        match self.repository_type {
            RepositoryType::MultiFile => {
                // Ensure that the repository path exsits and is a folder
                if !self.repo.exists() {
                    return Err(anyhow!(
                        "Attempted to open a repository at a path that does not exist."
                    ));
                }
                let md = metadata(&self.repo).with_context(|| {
                    format!(
                        "IO error when attempting to open MultiFile at {:?}",
                        &self.repo
                    )
                })?;
                if !md.is_dir() {
                    return Err(anyhow!("Attempted to open a MultiFile repository, but the path provided was not a folder."));
                }

                // First, attempt to read the multifile key
                let multifile_key = multifile::MultiFile::read_key(&self.repo)
                    .with_context(|| "Error attempting to read MultiFile key material")?;

                // Attempt to decrypt the key
                let key = multifile_key
                    .decrypt(self.password.as_bytes())
                    .with_context(|| {
                        "Unable to decrypt key material, possibly due to an invalid password"
                    })?;

                // Actually open the repository, and wrap it in a dynamic backend
                let chunk_settings = self.get_chunk_settings();
                let multifile = multifile::MultiFile::open_defaults(
                    &self.repo,
                    Some(chunk_settings),
                    &key,
                    queue_depth,
                )
                .await
                .with_context(|| "Exeprienced an internal backend error.")?;
                Ok((multifile.get_object_handle(), key))
            }
            RepositoryType::FlatFile => {
                // First, make sure the repository exists and is a file
                if !self.repo.exists() {
                    return Err(anyhow!(
                        "Attempted to open a repository path that does not exist"
                    ));
                }
                let md = metadata(&self.repo).with_context(|| {
                    format!(
                        "IO error when attempting to open FlatFile at {:?}",
                        &self.repo
                    )
                })?;
                if !md.is_file() {
                    return Err(anyhow!("Attempted to open a FlatFile repository, but the path provided was not a folder"));
                }

                // Attempt to open up the flatfile backend
                let chunk_settings = self.get_chunk_settings();
                // Attempt to read and decrypt the key
                let key = flatfile::FlatFile::load_encrypted_key(&self.repo)
                    .with_context(|| "Failed to read key from flatfile.")?;
                let key = key.decrypt(self.password.as_bytes()).with_context(|| {
                    "Unable to decrypt key material, possibly due to an invalid password"
                })?;
                let flatfile = flatfile::FlatFile::new(
                    &self.repo,
                    Some(chunk_settings),
                    None,
                    key.clone(),
                    queue_depth,
                )
                .with_context(|| "Internal backen d error opening flatfile.")?;
                let flatfile = flatfile.get_object_handle();
                Ok((flatfile, key))
            }
            RepositoryType::SFTP => {
                use asuran::repository::backend::sftp::*;
                let repo_str = self.repo.to_str().context("Non utf-8 in sftp path")?;
                let (username, hostname, path) = parse_ssh_path(repo_str)?;
                let settings = SFTPSettings {
                    hostname,
                    port: self.sftp_port,
                    username,
                    password: self.sftp_password.clone(),
                    path,
                };
                let key = SFTP::read_key(settings.clone())
                    .context("Unable to read repository key material")?
                    .decrypt(self.password.as_bytes())
                    .context(
                        "Failed to decrypt key material, possibly due to an invalid password",
                    )?;
                let chunk_settings = self.get_chunk_settings();
                let sftp = SFTP::connect(settings, key.clone(), Some(chunk_settings), queue_depth)
                    .context("Failed to connect to SFTP backend")?;
                Ok((sftp.get_object_handle(), key))
            }
        }
    }
}

/// Takes a string of type user@host:/path, with optional user, and returns a tuple of strings of
/// rom (user, host, path). Will default to the username this program is running as
///
/// Will return an error if this command is not of a valid format
///
/// # Example:
///
/// ```rust
/// let path = "username@hostname:/path/of/the/thing";
/// let (username,hostname,path) = parse_ssh_path(path).unwrap();
/// assert_eq!(username,"username");
/// assert_eq!(hostname,"hostname");
/// assert_eq!(path,"/path/of/the/thing");
/// ```
pub fn parse_ssh_path(input: &str) -> Result<(String, String, String)> {
    // First split into a host/path part We use splitn with a max of two parts, so if by some chance
    // the user has a colon in the provided path, it doesn't blow up
    let parts = input.splitn(2, ':').collect::<Vec<_>>();
    // Make sure there is both a host part and a path part
    if parts.len() != 2 {
        return Err(anyhow!(
            "Provided sftp path either does not contain a host part or a path part"
        ));
    }
    // extract the path part
    let path = parts[1].to_string();
    // Parse the username/host part
    let host_parts = parts[0].splitn(2, '@').collect::<Vec<_>>();
    // Return an error if the host part was empty
    if host_parts.is_empty() {
        return Err(anyhow!("No hostname was provided."));
    }
    let (username, hostname) = if host_parts.len() == 1 {
        // Attempt to get user's username in a janky but cross platform way
        // *nix has the USER env variable, and windows has USERNAME.
        // We just try them both, in that order, and fail if neither returns.
        let username = env::var_os("USER")
            .or_else(|| env::var_os("USERNAME"))
            .with_context(|| {
                "Unable to determine username automatically, please specify a username manually."
            })?
            .to_str()
            .with_context(|| {
                "OS Provided username contained non-UTF8, please specify a username manually"
            })?
            .to_string();
        (username, host_parts[0].to_string())
    } else {
        // Username was provided, use that
        (host_parts[0].to_string(), host_parts[1].to_string())
    };

    Ok((username, hostname, path))
}
