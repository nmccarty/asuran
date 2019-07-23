### Overview

Asuran uses an archive format roughly inspred by borg.
Asuran aims to provided the security and deduplication performance of borg or restic, while providing the user friendliness and cloud backend support of Duplicati.

### Encryption

The encryption backend is plugable, it currently only supports AES-CBC and AES-CTR in the 256 bit variants, but support for AES-GCM, 128 bit, and other cipher sets is in the works.
Encryption can also be turned off, in case the backend is trusted and extra peformance is desired.

### Compression

The compression backend is also plugable. Currently supported is ZStd, support for lz4, lzma, and deflate are in the works.
Compression can also be turned off, and a mode that turns off compression for uncompressible data is in the works.

### Authentication

Both the data itself, as well as the context (through the borg-style object graph) are authenticated through HMAC with pluggable hashing algorthims.
Currently, only SHA256 and Blake2b are supported. 

Incase you don't trust my roll-your-own AEAD implementation, AES-GCM support using an audited external library and potential nonce tracking is a high priority.

## libasuran

Repositories are interacted with soley though the high-level API provied by libasuran, allowing ausran support to be easily built into other applications.