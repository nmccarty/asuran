use asuran::prelude::*;
use tempfile::tempdir;

async fn create_multifile_repository(encryption: Encryption, compression: Compression, hmac: HMAC) {
    let directory = tempdir().expect("Unable to open temporary directory.");

    let repo_dir = directory.path();

    let key = Key::random(encryption.key_length());
    let encrypted_key = EncryptedKey::encrypt_defaults(
        &key,
        encryption,
        "And then there was silence, Just a voice from the other world".as_bytes(),
    );

    let settings = ChunkSettings {
        compression,
        encryption,
        hmac,
    };

    let mut mf = MultiFile::open_defaults(repo_dir, Some(settings), &key, 4)
        .await
        .expect("Unable to create the multifile repository");
    mf.write_key(&encrypted_key)
        .await
        .expect("Unable to write encrypted key");
    mf.close().await;
}

// Attempts to create a multifile repository with no encryption.
// Specifically addresses gitlab issue #56
#[test]
fn create_multifile_noencryption() {
    smol::run(async {
        create_multifile_repository(
            Encryption::NoEncryption,
            Compression::NoCompression,
            HMAC::Blake3,
        )
        .await
    });
}

// Attempts to reproduce gitlab issue #58
#[test]
fn create_lzma_9() {
    smol::run(async {
        create_multifile_repository(
            Encryption::new_aes256ctr(),
            Compression::LZMA { level: 9 },
            HMAC::Blake3,
        )
        .await
    });
}
