use crate::util::DynamicBackend;
use anyhow::{anyhow, Context, Result};
use asuran::repository::{self, Backend, Key};
use clap::{arg_enum, AppSettings};
use repository::backend::{flatfile, multifile};
use std::fs::metadata;
use std::path::PathBuf;
use structopt::StructOpt;

const VERSION: &'static str = concat!(
    env!("VERGEN_SEMVER"),
    "-",
    env!("VERGEN_SHA_SHORT"),
    " ",
    env!("VERGEN_BUILD_DATE"),
);

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

impl Opt {
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
            Encryption::AES256CBC => repository::Encryption::new_aes256cbc(),
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
    pub async fn open_repo_backend(&self) -> Result<(DynamicBackend, Key)> {
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
                    .with_context(|| format!("Error attempting to read MultiFile key material"))?;

                // Attempt to decrypt the key
                let key = multifile_key
                    .decrypt(self.password.as_bytes())
                    .with_context(|| {
                        "Unable to decrypt key material, possibly due to an invalid password"
                    })?;

                // Actually open the repository, and wrap it in a dynamic backend
                let chunk_settings = self.get_chunk_settings();
                let multifile =
                    multifile::MultiFile::open_defaults(&self.repo, Some(chunk_settings), &key)
                        .with_context(|| "Exeprienced an internal backend error.")?;
                Ok((DynamicBackend::from(multifile), key))
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
                let flatfile = flatfile::FlatFile::new(&self.repo, Some(chunk_settings), None)
                    .with_context(|| "Internal backend error opening flatfile.")?;
                let flatfile = DynamicBackend::from(flatfile);

                // Attempt to read and decrypt the key
                let key = flatfile
                    .read_key()
                    .await
                    .with_context(|| "Failed to read key from flatfile.")?;
                let key = key.decrypt(self.password.as_bytes()).with_context(|| {
                    "Unable to decrypt key material, possibly due to an invalid password"
                })?;
                Ok((flatfile, key))
            }
        }
    }
}
