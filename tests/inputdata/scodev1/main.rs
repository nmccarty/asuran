use libasuran::chunker::Chunker;
use libasuran::manifest::archive::*;
use libasuran::repository::backend::filesystem::*;
use libasuran::repository::compression::Compression;
use libasuran::repository::encryption::Encryption;
use libasuran::repository::hmac::HMAC;
use libasuran::repository::Repository;
use rand::prelude::*;
use std::fs;
use std::io::Write;
use std::io::{BufReader, Cursor};
use std::path::Path;
use tempfile::tempdir;

#[cfg(feature = "profile")]
use flame::*;
#[cfg(feature = "profile")]
use std::fs::File;

fn single_add_get(seed: u64) -> bool {
    println!("Seed: {}", seed);
    let chunker = Chunker::new(48, 23, 0);

    let key: [u8; 32] = [0u8; 32];
    let size = 100 * 2_usize.pow(20);
    #[cfg(feature = "profile")]
    flame::start("fill data");
    let mut data = vec![0_u8; size];
    let mut rand = SmallRng::seed_from_u64(seed);
    rand.fill_bytes(&mut data);
    #[cfg(feature = "profile")]
    flame::end("fill data");
    let root_dir = tempdir().unwrap();
    let root_path = root_dir.path().display().to_string();

    let backend = FileSystem::new_test(&root_path);
    let mut repo = Repository::new(
        backend,
        Compression::NoCompression,
        HMAC::Blake2b,
        Encryption::NoEncryption,
        &key,
    );

    let mut archive = Archive::new("test");

    let testdir = tempdir().unwrap();
    let input_file_path = testdir.path().join(Path::new("file1"));
    {
        let mut input_file = fs::File::create(input_file_path.clone()).unwrap();
        input_file.write_all(&data).unwrap();
    }
    let mut input_file = BufReader::new(fs::File::open(input_file_path).unwrap());

    #[cfg(feature = "profile")]
    flame::start("put object");
    archive
        .put_object(&chunker, &mut repo, "FileOne", &mut input_file)
        .unwrap();
    #[cfg(feature = "profile")]
    flame::end("put object");

    #[cfg(feature = "profile")]
    flame::start("get object");
    let mut buf = Cursor::new(Vec::<u8>::new());
    archive.get_object(&repo, "FileOne", &mut buf).unwrap();
    #[cfg(feature = "profile")]
    flame::end("get object");

    let output = buf.into_inner();
    println!("Input length: {}", data.len());
    //   println!("Input: \n{:X?}", data);
    println!("Output length: {}", output.len());
    //    println!("Output: \n{:X?}", output);

    let mut mismatch = false;
    for i in 0..data.len() {
        if data[i] != output[i] {
            println!(
                "Byte {} was different in output. Input val: {:X?} Output val {:X?}",
                i, data[i], output[i]
            );

            mismatch = true;
        }
    }

    !mismatch
}

fn main() {
    #[cfg(feature = "profile")]
    println!("hello");
    #[cfg(feature = "profile")]
    flame::start("single_add_get");
    single_add_get(1);
    #[cfg(feature = "profile")]
    flame::end("single_add_get");
    #[cfg(feature = "profile")]
    flame::dump_stdout();
    //    #[cfg(feature = "profile")]
    //    flame::dump_html(&mut File::create("flame-graph.html").unwrap()).unwrap();
}
