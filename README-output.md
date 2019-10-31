
# Table of Contents

1.  [Overview](#org86d370c)
2.  [Features Overview](#org579878c)
    1.  [Deduplication](#orgcaad4e9)
    2.  [Encryption](#orgddf34dd)
        1.  [Supported Encryption types](#org425b3ba)
        2.  [Key generation and storage](#org9e90eb7)
    3.  [Compression](#org821d0e9)
        1.  [Supported Compression types](#org20bc088)
        2.  [Intelligent Compression](#orge9c2bc9)
    4.  [Authentication](#orgfb2eb77)
        1.  [Supported Hashes for HMAC](#org0d278c3)
3.  [Internals Overview](#orge1b7587)
    1.  [Repository](#org076acfc)
        1.  [Chunks](#org060a0dd)
        2.  [Repository Backend](#orga1de195)
    2.  [Manifest](#org863c48a)
        1.  [Archive](#orgaff1598)
        2.  [Targets](#org62782ed)
        3.  [Target Drivers](#org1e00417)
    3.  [Chunker](#org370266b)
        1.  [BuzHash](#orgceefbdd)
        2.  [Static Size](#org0ae2b67)
        3.  [Disk Image Chunker](#orgf6a9534)
4.  [Development Process / Contributing](#org8166b37)
    1.  [Roadmap](#orgb1c5b2e)
        1.  [0.1.0](#orgd3ddf4c)
        2.  [0.2.0](#org0239786)
5.  [Inspiration/Motivation](#orgba145b2)
    1.  [Features Borg has that Restic is missing](#orgaa3cc7c)
    2.  [Features Restic has that Borg is missing](#orga92a989)
    3.  [Features I want that neither has](#org22aceb1)
    4.  [Comparison with rdedup](#orgc7cc9b8)



<a id="org86d370c"></a>

# Overview

Asuran is a new archive format, and an implementation (in rust) of both a read/write library as
well as a command line tool.


<a id="org579878c"></a>

# Features Overview


<a id="orgcaad4e9"></a>

## Deduplication

Deduplication is achieved through a combination of content defined slicing and content
addressable storage. The CAS backend will only store each chunk submitted to it once. The content
defined slicing provides a reasonable assurance that objects will be broken up into chunks in
shuch a way that duplicate chunks will occur, if possible, and not be stored twice.


<a id="orgddf34dd"></a>

## Encryption

The encryption backend is plugable and can be changed on a chunk-by-chunk basis. Changing the
default encryption will only change the encryption method for new chunks.

Each chunk is encrypted and then put in an Object along side a tag indicating the encryption
algorithm used, as well as the IV if the algorithm requires one.


<a id="org425b3ba"></a>

### TODO Supported Encryption types

The supported encryption types are as follows. All types are authenticated with an HMAC tag,
whether or not the algorithm includes authentication.

1.  [X] No Encryption (Passthrough cipher)
2.  [X] AES256-CBC
3.  [X] AES256-CTR
4.  [X] AES256-GCM
5.  [ ] chacha20-poly1305
6.  [ ] Twofish
7.  [ ] Serpent


<a id="org9e90eb7"></a>

### Key generation and storage

They keys used by the encryption and HMAC are generated randomly at repository creation, and are
stored on disk encrypted with a key derived from a user supplied passphrase. The current KDF
used is bcrypt, however this will be swapped out for a more modern one in the near future, most
likely argon2id.


<a id="org821d0e9"></a>

## Compression

The compression backend is pluggable and supports a variety of compression
algorithms. Compression takes place before encryption, and each chunk is tagged with the
compression algorithm used to compress it<sup><a id="fnr.1" class="footref" href="#fn.1">1</a></sup>.


<a id="org20bc088"></a>

### TODO Supported Compression types

Supported Compression types are as follows

1.  [X] No Compression
2.  [X] ZStd
3.  [ ] Zlib
4.  [ ] LZO
5.  [ ] LZ4
6.  [ ] BZip
7.  [ ] LZMA


<a id="orge9c2bc9"></a>

### TODO Intelligent Compression

Asuran will support (but currently does not) intelligent encrypt on, where it will do a trial
compression of the first chunk or chunks of a file with lz4, and if reasonable compression is
detected, use the selected compression algorithm too compress the chunks of the file, otherwise
applying no compression.


<a id="orgfb2eb77"></a>

## Authentication

All data stored in the repository is authenticated through an encrypt-then-mac construction, with
HMAC-SHA256 and Blake2b currently supported. Asuran will sternly refuse to unpack a chunk if the
HMAC does not pass verification.

Content keys used by the CAS are generated with an HMAC of the plain text, using a different key
than the HMAC used for verification. These are currently not verified.


<a id="org0d278c3"></a>

### TODO Supported Hashes for HMAC

The supported HMAC algorithms are as follows

1.  [X] SHA256
2.  [X] Blake2b
3.  [ ] SHA3
4.  [ ] Blake2s
5.  [ ] Whirlpool
6.  [ ] MD6


<a id="orge1b7587"></a>

# Internals Overview


<a id="org076acfc"></a>

## Repository

The repository is a low level key-value store that is commited to disk. All values (refered to
as "chunks") in the repository are encrypted, compressed, and HMACed with algorithms
configurable on a per-value basis.

The repository only understands keys and values, and effectively operates as content addressable
storage, all other data structures are implemented on top of the repository.

The repository structure itself is storage-independent. The repository object itself simply views
the world as list of segments, which themselves are lists of sized cells containing values.

Repositories are not strictly required to have multiple segments, and segments are not strictly
required to contain multiple chunks. This allows simple mapping of any random access storage as
(possibly) a single segment, or an object type store (such as S3) as a number of segments each
containing one or many chunks.

The repository has special methods for pulling the manifest and index out of itself, and it may
or may not treat these pieces of data as special, depending on the backend implementation in
use. Typically, the manifest will be stored as a normal Chunk with a special key that is all
zero.


<a id="org060a0dd"></a>

### Chunks

A chunk is the representation of a value in the repository.

It is a compressed and encrypted sequence of bytes, along with a set of tags describing the
encryption, compression, and HMAC algorithms used, as well as any IVs those algorithms require.

Chunks contain two HMAC values, id and hmac.

Compression and encryption are swappable on a per chunk basis.

1.  TODO ID

    The ID of the Chunk is the HMAC of its plain text content, ideally using a different key than
    hmac, but currently uses the same key. (Will be changed in a future version).
    
    ID is used for deduplication, and is the key used to reference the chunk in the repository.

2.  HMAC

    The HMAC of the chunk is, as the name implies, an HMAC of the chunk's encrypted contents. This
    is used for authentication and data integrity verification.


<a id="orga1de195"></a>

### Repository Backend

The repository backend is responsible for translating the repository's "list of lists"
segment/chunk view onto whatever storage backend is desired.  The backend is additionally
responsible for providing a map from keys to (Segment, Offset) pairs.

Segments are stored as the concatenation of the bytes making up the MessagePack representation
of their chunks.

As long as the methods return what they should, Asuran places no restrictions on how the
underlying mapping occurs, or what side effects the methods should perform.

These methods are extremely likely to be side effect prone in any implementation, and,
generally, should not be called directly by the consumer, and instead used indirectly through
the repository API.

1.  Filesystem Backend

    The filesystem back end uses a configurable segment size<sup><a id="fnr.2" class="footref" href="#fn.2">2</a></sup>, storing segments in folders
    with a configurable limit on the number of segments in a folder<sup><a id="fnr.3" class="footref" href="#fn.3">3</a></sup> (to avoid filesystem
    operations bogging down).


<a id="org863c48a"></a>

## Manifest

The manifest is the root of the repository's object graph, and is the primary object through
which repository access is managed.

The manifest contains a list of reference to Archive objects within the repository, as well as
methods for managing them. The manifest also contains utility methods, that when paired with a
Target driver, can be used to backup objects to and restore objects from a repository.

The manifest additionally contains a timestamp of its last modification, as well as the ability
to load and commit itself from/to the repository.


<a id="orgaff1598"></a>

### Archive

An archive is conceptually a collection of objects stored in a repository.  This is the most
common entry and exit point for data.

An archive object contains a name<sup><a id="fnr.4" class="footref" href="#fn.4">4</a></sup>, a list of the objects in the archive (stored as a
HashMap mapping the path of the object to a list of its chunks and the offsets of the chunk
within the object), as well as the timestamp of the archive's creation.

The timestamp is primarily intended to prevent replay attacks, but also serves to provide the
user with additional information, as well as allowing the user to distinguish multiple archives
with identical names.

Object paths are unix-path style "/" delimited lists of tokens, and while they usually will map
directly to paths, they are not required to, thus the individual tokens are allowed to contain
any unicode character except "/".  The interpretation of the paths is left up to the target
driver.

Archives are commited to a manifest by MessagePacking them and storing the result as a Chunk in
the repository. The resulting ID is then wrapped in a Stored Archive object alongside the
metadata (name, creation date, etc&#x2026;), and the StoredArchive is then added to the manifest
list.

1.  Namespaces

    Archives are namespaced, allowing multiple objects with the same path to be contained in the
    same archive, so long as they are in different namespaces.
    
    Namespaces are described as colon delimited lists of tokens with a trailing colon, in order of
    increasing specificity (e.g. 1:2: would describe a namspace named "2" inside of a namespace
    named "1").
    
    The complete path of a specific object in a repository is described by appending the path of
    the object to its namespace string. For example, a file "/usr/share/example" stored in the root
    namespace of an archive would be referenced by the string ":/usr/share/example", where as the
    file's metadata might be referenced by "metadata:/usr/share/example", and auditing information
    might be referenced by "metadata:audit:/user/share/example".


<a id="org62782ed"></a>

### Targets

Targets abstract the operation of creating and restoring archive to/from various types of
storage. The API is written primarily to cater to the typical "files stored on a filesystem" use
case, but is by no means limited to it.

As long as the target storage has objects, that can be serialized into a byte stream, and the
"location" of those objects can be mapped to unix path style strings, then a valid target
implementation can be written for the storage.

1.  BackupTarget and RestoreTarget

    BackupTarget and RestoreTarget are the traits that targets must be able to implement in order
    to backup data to and restore data from an archive, respectively. Most, if not all, targets will
    implement both traits.
    
    1.  BackupTarget
    
        BackupTarget contains the following methods:
        
        1.  Paths
            Returns a list of objects to be stored, as well as their paths
        2.  Object
            Returns a reader for the object given its path (from the Paths method)
        3.  Listing
            Returns a serialized listing of all the objects stored. Typically
            stored in the archive at "archive:listing"
    
    2.  RestoreTarget
    
        RestoreTarget contains the following methods:
        
        1.  Load listing
            Parses the listing produced by BackupTarget::Paths
        2.  Object
            Returns a writer to the object's real location on the storage
        3.  Listing
            Provides a list of all paths to be restore.

2.  TODO Sparse Data

    The target API is written to support the concept of sparse data, but currently no targets
    actually have support for sparse data.
    
    Once complete, dense data will just be handled as the degenerate case of sparse data that has
    only one contiguous chunk. This will be implemented through describing BackupObjects and
    RestoreObjects as lists of pre-seeked readers and writers, and dense data will simply be the
    case where those lists only have one element.


<a id="org1e00417"></a>

### TODO Target Drivers

The target driver trait specifies a collection of methods for writing objects to and reading
objects from storage. The driver should handle the process of reading and writing the objects in
their entirety, with the consumer only having to supply the repository, the archive, the root
path to restore relative too, and the target object path.


<a id="org370266b"></a>

## Chunker

The chunker is responsible for dividing objects into chunks of bytes, using some well-defined
method.

The chunker framework is pluggable, and while support is planned for several chunkers, both
special and general purpose, is planned, currently Asuran only implements one, a content defined
chunker based on the BuzHash algorithm.


<a id="orgceefbdd"></a>

### BuzHash

The buzhash chunker used a modification of the buzhash rolling hash algorithm to perform content
defined slicing.

It runs a rolling hash down the data, and slices it when the last *n* bits of the hash are 0, as
long as other requirements are met.

This chunker has three settings:

1.  Window Size
    Adjusts the sizof the data window considered by the rolling hash
2.  Mask Bits
    How many bits of the hash have to be 0 to determine a slice.
    
    With a Mask Bits value of *n*, the chunker will not split the data if it would result in a
    chunk less than 2<sup>*n* - 2</sup> bytes in size, and will always split the data if the chunk is
    about to exceed 2<sup>*n* + 2</sup> in size
3.  Nonce 
    This implementation randomizes the buzhash table to help prevent chunk size based
    fingerprinting attacks. The Nonce is the seed used for the random number generator that fills
    the table.


<a id="org0ae2b67"></a>

### TODO Static Size

The static size chunker will always may the chunks the same, configurable, size


<a id="orgf6a9534"></a>

### TODO Disk Image Chunker

The disk image chunker will understand disk image formats, and chunk them in an intelligent way.

1.  TODO Raw Image Chunker

    The raw image chunker will attempt to detect raw disk images (e.g. iso, img, etc..) and put any
    metadata in its own chunks, and then attempt to make the chunk size match up with the block
    size of the image.

2.  TODO VMA Chunker

    This chunker should understand the Proxmox VMA format and be able to chunk it intelligently to
    maximize dedeuplication.


<a id="org8166b37"></a>

# Development Process / Contributing

As it is only me developing at the moment, the current development model isn't very structured. In
the future it will consist of a branch-per-featured model with branches being required to past a
minimum set of tests before being merged into master.

Pull requests and issues are welcome, and by contributing to this project you agree to license
your work under the MIT license.


<a id="orgb1c5b2e"></a>

## Roadmap


<a id="orgd3ddf4c"></a>

### 0.1.0

Release 0.1.0 should be a somewhat usable product. It will still only operate in append only
mode, but will have support for an array of encryption, compression, and HMAC algorithm
types. It will additionally have a tentatively stabilized on-disk format. The repository should
be able to verify itself as a dedicated operation. The filesystem target should handle sparse
data correctly.

1.  TODO libasuran

    libasuran 0.1.0 should have the following features:
    
    -   [ ] Somewhat stable on disk format
    -   [ ] Support for zlib, lzma, and lz4 compression
    -   [ ] Support for chacha20-poly1305 encryption
    -   [ ] Should have cargo benchmarks
    -   [ ] Should have a working sparse data API
    -   [ ] Should have a method for verifiying the integreity of the repo

2.  TODO asuran

    asuran 0.1.0 should have the following features:
    
    -   [ ] Support for setting compression type/level
    -   [ ] Support for setting encryption type
    -   [ ] Support for setting HMAC algorithm
    -   [ ] Runtime tests/benchmarks
    -   [ ] Repository verification command


<a id="org0239786"></a>

### 0.2.0


<a id="orgba145b2"></a>

# Inspiration/Motivation

This project is inspired by both [Borg](https://borgbackup.readthedocs.io/en/stable/) and [Restic](https://restic.net/). Both are very good pieces of software, and
perfectly suitable for many use cases, but my use case seems to lie in between the two.

In many ways, this project is intended to be a mashup of what I consider to be the best features
of the two applications, while attempting to make a modifiable and extendable framework that can
be embedded in other applications easily. 


<a id="orgaa3cc7c"></a>

## Features Borg has that Restic is missing

-   Performance
    Borg generally has way better performance than Restic, in my work load I have personally found
    this to be to a disturbing extent.
-   Optional/Switchable Encryption
    Don't get me wrong, being able to safely store sensitive data on untrusted storage is really
    nice, but sometimes you really are backing up to trusted storage (e.g. an external hard drive
    that is already encrypted at the file system or drive level), and double encryption is just
    extra overhead.
-   Optional/Switchable Compression
    Restic doesn't support compression at all, which, in my opinion, makes it a no-go for many
    workloads


<a id="orga92a989"></a>

## Features Restic has that Borg is missing

-   Switchable Storage Backends
    This one is a big deal for me. As a home gamer, being able to directly backup my datahoarder
    levels of files to an unlimited GDrive or the like is a huge deal. This is also the *only*
    reason I use Restic for some of my backups
-   Multiple computers writing to the same repository
    Borg's repository locking and chunk cache mechanisms make writing to the same repository with
    multiple computers a huge pain in the ass. Not having all your computers backing up to the same
    repository decreases deduplication by an extremely large factor and is just generally not good.


<a id="org22aceb1"></a>

## Features I want that neither has

-   Tar import and export
    
    This isn't entirely true, borg has tar export and is working on tar import, but it lacks one
    feature that is critical to my workflow, reproducing the same tar file. My work flow involves a
    program that produces backups as tar files, and when restoring them looks for a special file
    that must be the first in the tar. I would like the ability to import and export tars and keep
    the metadata of the tar the same, while still being able to take the tar apart and deduplciate
    the individual files within it, and use the compression defined by the repository.
-   Good multithreading
    
    While borg is python based and doesn't really used threads, restic has multithreading, but in
    my opinion, doesn't use it well


<a id="orgc7cc9b8"></a>

## Comparison with [rdedup](https://github.com/dpc/rdedup)

rdedup is a very good tool, but falls sort in several areas for me. 

-   No built in directory traversal
    
    rdedup depends on external tools like tar to make backups. In my experience this makes for a
    poor deduplication rate compared to borg in my workflow.

-   No current support for cloud backends
    
    This one is almost cheating because asuran does not currently have support for cloud backends,
    but asuran was designed from the ground up to be storage-agnostic.

-   No intelligent chunking
    
    rdedup has good support for choosing from a few good content defined slicers, but lacks the
    framework for intelligent slicing of known data types, such as disk images that can be sliced
    blockwise, or intelligent picking apart of backups emitted by other applications in a way to
    maximize deduplication.

-   Little/no integration support
    
    This complaint also somewhat applies to borg and restic, but to a lesser extent. libasuran is
    designed to be called into from other applications, such as a carbonite style automatic backup
    utility, allowing the easy creation of end user friendly applications that support the full
    suite of asuran features.


# Footnotes

<sup><a id="fn.1" href="#fnr.1">1</a></sup> The compression level used is also included in this tag, regardless of if it is needed or not.

<sup><a id="fn.2" href="#fnr.2">2</a></sup> Currently 250kB by default

<sup><a id="fn.3" href="#fnr.3">3</a></sup> Currently 250 segments per folder by default

<sup><a id="fn.4" href="#fnr.4">4</a></sup> A name can be any arbitrary string, and does not need to be unique.
