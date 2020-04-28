use asuran::chunker::*;
use asuran::manifest::*;
use asuran::repository::*;
use rand::prelude::*;
use std::io::Cursor;
use tempfile::tempdir;

mod common;

#[test]
fn put_drop_get_multifile() {
    smol::run(async {
        let tempdir = tempdir().unwrap();
        let root_path = tempdir.path().to_str().unwrap();
        let key = Key::random(32);
        let mut repo = common::get_repo_bare(root_path, key.clone()).await;

        let chunker = FastCDC::default();

        let mut objects: Vec<Vec<u8>> = Vec::new();

        for _ in 0..5 {
            let mut object = vec![0_u8; 16384];
            thread_rng().fill_bytes(&mut object);
            objects.push(object);
        }

        {
            let mut manifest = Manifest::load(&mut repo);
            manifest
                .set_chunk_settings(repo.chunk_settings())
                .await
                .unwrap();
            let mut archive = ActiveArchive::new("test");
            for (i, object) in objects.iter().enumerate() {
                archive
                    .put_object(
                        &chunker,
                        &mut repo,
                        &i.to_string(),
                        Cursor::new(object.clone()),
                    )
                    .await
                    .unwrap();
            }
            println!("Archive: \n {:?}", archive);
            manifest.commit_archive(&mut repo, archive).await.unwrap();
            println!("Manifest: \n {:?}", manifest);
        }
        repo.close().await;
        let mut repo = common::get_repo_bare(root_path, key).await;

        let mut manifest = Manifest::load(&mut repo);
        let archive = manifest.archives().await[0].load(&mut repo).await.unwrap();
        for (i, object) in objects.iter().enumerate() {
            let mut buffer = Cursor::new(Vec::<u8>::new());
            println!("Archive: \n {:?}", archive);
            archive
                .get_object(&mut repo, &i.to_string(), &mut buffer)
                .await
                .unwrap();
            let buffer = buffer.into_inner();
            assert_eq!(object, &buffer);
        }
    });
}

#[test]
fn put_drop_get_mem() {
    smol::run(async {
        let key = Key::random(32);
        let mut repo = common::get_repo_mem(key);

        let chunker = FastCDC::default();

        let mut objects: Vec<Vec<u8>> = Vec::new();

        for _ in 0..5 {
            let mut object = vec![0_u8; 16384];
            thread_rng().fill_bytes(&mut object);
            objects.push(object);
        }

        {
            let mut manifest = Manifest::load(&mut repo);
            manifest
                .set_chunk_settings(repo.chunk_settings())
                .await
                .unwrap();
            let mut archive = ActiveArchive::new("test");
            for (i, object) in objects.iter().enumerate() {
                archive
                    .put_object(
                        &chunker,
                        &mut repo,
                        &i.to_string(),
                        Cursor::new(object.clone()),
                    )
                    .await
                    .unwrap();
            }
            println!("Archive: \n {:?}", archive);
            manifest.commit_archive(&mut repo, archive).await.unwrap();
            println!("Manifest: \n {:?}", manifest);
        }

        let mut manifest = Manifest::load(&mut repo);
        let archive = manifest.archives().await[0].load(&mut repo).await.unwrap();
        for (i, object) in objects.iter().enumerate() {
            let mut buffer = Cursor::new(Vec::<u8>::new());
            println!("Archive: \n {:?}", archive);
            archive
                .get_object(&mut repo, &i.to_string(), &mut buffer)
                .await
                .unwrap();
            let buffer = buffer.into_inner();
            assert_eq!(object, &buffer);
        }
    });
}
