use async_std::task::block_on;
use libasuran::chunker::slicer::fastcdc::FastCDC;
use libasuran::chunker::*;
use libasuran::manifest::*;
use libasuran::repository::*;
use rand::prelude::*;
use std::io::{Cursor, Empty};
use tempfile::tempdir;

mod common;

#[test]
fn put_drop_get_filesytem() {
    let tempdir = tempdir().unwrap();
    let root_path = tempdir.path().to_str().unwrap();
    let key = Key::random(32);
    let mut repo = common::get_repo(root_path, key);

    let slicer: FastCDC<Empty> = FastCDC::new_defaults();
    let chunker = Chunker::new(slicer.copy_settings());

    let mut objects: Vec<Vec<u8>> = Vec::new();

    for _ in 0..5 {
        let mut object = vec![0_u8; 16384];
        thread_rng().fill_bytes(&mut object);
        objects.push(object);
    }

    {
        let mut manifest = Manifest::load(&repo);
        manifest.set_chunk_settings(repo.chunk_settings());
        let mut archive = Archive::new("test");
        for (i, object) in objects.iter().enumerate() {
            block_on(archive.put_object(
                &chunker,
                &mut repo,
                &i.to_string(),
                &mut Cursor::new(object),
            ))
            .unwrap();
        }
        println!("Archive: \n {:?}", archive);
        manifest.commit_archive(&mut repo, archive);
        println!("Manifest: \n {:?}", manifest);
    }

    let manifest = Manifest::load(&repo);
    let archive = block_on(manifest.archives()[0].load(&repo)).unwrap();
    for (i, object) in objects.iter().enumerate() {
        let mut buffer = Cursor::new(Vec::<u8>::new());
        println!("Archive: \n {:?}", archive);
        block_on(archive.get_object(&repo, &i.to_string(), &mut buffer)).unwrap();
        let buffer = buffer.into_inner();
        assert_eq!(object, &buffer);
    }
}

#[test]
fn put_drop_get_mem() {
    let key = Key::random(32);
    let mut repo = common::get_repo_mem(key);

    let slicer: FastCDC<Empty> = FastCDC::new_defaults();
    let chunker = Chunker::new(slicer.copy_settings());

    let mut objects: Vec<Vec<u8>> = Vec::new();

    for _ in 0..5 {
        let mut object = vec![0_u8; 16384];
        thread_rng().fill_bytes(&mut object);
        objects.push(object);
    }

    {
        let mut manifest = Manifest::load(&repo);
        manifest.set_chunk_settings(repo.chunk_settings());
        let mut archive = Archive::new("test");
        for (i, object) in objects.iter().enumerate() {
            block_on(archive.put_object(
                &chunker,
                &mut repo,
                &i.to_string(),
                &mut Cursor::new(object),
            ))
            .unwrap();
        }
        println!("Archive: \n {:?}", archive);
        manifest.commit_archive(&mut repo, archive);
        println!("Manifest: \n {:?}", manifest);
    }

    let manifest = Manifest::load(&repo);
    let archive = block_on(manifest.archives()[0].load(&repo)).unwrap();
    for (i, object) in objects.iter().enumerate() {
        let mut buffer = Cursor::new(Vec::<u8>::new());
        println!("Archive: \n {:?}", archive);
        block_on(archive.get_object(&repo, &i.to_string(), &mut buffer)).unwrap();
        let buffer = buffer.into_inner();
        assert_eq!(object, &buffer);
    }
}
