Asuran
======

We believe that backups should be easy, fast, and last forever.

Asuran is a new archive format and rust implementation. It aims to be the archiver for the 2020's, and has been written from the ground up to use the insights from cutting edge research and extract every last bit of performance out of modern hardware, while still providing features users have come to rely on in archivers, like encryption, compression, and global deduplication.

Asuran is also blazing fast, [check out some benchmarks!](https://gitlab.com/asuran-rs/archiver-benchmarks/-/blob/master/RESULTS.md)

![Codecov](https://img.shields.io/codecov/c/gl/asuran-rs/asuran?style=flat-square) ![Gitlab pipeline status
(branch)](https://img.shields.io/gitlab/pipeline/asuran-rs/asuran/master?style=flat-square) ![Crates.io](https://img.shields.io/crates/v/asuran?style=flat-square) ![Crates.io](https://img.shields.io/crates/l/asuran?style=flat-square)

A note on stability
-------------------

Asuran and asuran-cli are *pre-alpha software*, prior to version 1.0.0, releases are for evaluation and testing only. Prior to 1.0.0, the API may make breaking changes between patch releases, and there may be breaking format changes between patch releases before 0.2.0 (after 0.2.0, breaking format changes may only happen between minor version increases). Please always read the changelog before updating.

Mission Statement
-----------------

Asuran should be suitable for the long term archival of data, should be operating system and hardware independent, secure, flexible, fast, and easily embeddable.

It should strive to make backups a fast and easy process, and to allow the user to preserve as much of their file history as possible in the space they have avaible. After all, what good is a backup that never ran because it would have taken too long, or the backup that got deleted because it was using too much space?

Asuran should be safe for use on untrusted storage, and should not leak any data that could reveal, to any extent, the contents of the repository.

How Does it Work?
-----------------

Asuran works by splitting your files up into a number of chunks. It splits files up using a selectable Content Defined Chunking algorithim, currently FastCDC by default, so that even if one part of a file changes, it is very likely the other chunks will not..

Those chunks are then optionally (but on by default) compressed and encrypted before being committed to a Content Addressable Storage backend for later retrievial. Asuran tries its best to store each chunk it encounters only once in the repository, and in the most common usecases, it can do this 100% of the time.

The entire archive structure is verified with a Merkle Tree process, so you can be sure that if your restore succeeds, your data is intact and has not been tampered with.

You can store as many archives as you want in a single repository, and write to the same repository with as many computers as you want.

The entire data pipeline is built on top of a modern, async stack, allowing performance that previous contenders in this space could only have dreamed of.

Installing and using
--------------------

You can either download a copy from our [releases](https://gitlab.com/asuran-rs/asuran/-/releases) page, or build it from source yourself.

Please note, the builds on our releases page require a core-branded sandybridge or newer/equivalent, with required support for AVX instructions. If you need to run on a lower end CPU or an older machine without these instructions, you will need to build it yourself from source.

In most cases you will be interacting with the command line asuran client (asuran-cli). Either build it from source from the asuran-cli directory in this repository, or install it with:

```bash
cargo install asuran-cli
```

Optionally build with `env RUSTFLAGS="-C target-cpu=native -C target-feature=+aes" cargo install asuran-cli` for even better performance.

See `asuran-cli --help` for usage.

`asuran-cli` is, at heart, a thin wrapper that glues together the API of the `asuran` library. The `asuran` crate provides a high level interface for interacting with repositories, and will always be a sepereate component and enjoy the same level of support as `asuran-cli` itself.

Documentation
-------------

Please see our [RustDocs](https://asuran-rs.gitlab.io/asuran/asuran/) for api documentation, and the [Internals](https://www.asuran.rs/Internals.html) document for disucssion about the format.

Support
=======

Developing software is hard work, and continuing to improve asuran takes a substantial portion of my time.

I am currently working on getting a patreon/open collective/sponus or the like setup, but in the mean time, if you wish to support me, feel free to toss me your favorite cryptocurrency:

-	BTC: bc1q99tz5sv4mn9l3mhx3qc3lh64skgx85uxssg3tc
-	ETH: 0xd9CdBD945fE347FDAC4DFA71E13cB3EED7595882
-	XRP: r46gGdwgMVMaWreVbRzSoxm9QrT3uSoEWC
-	USDT: 0xd9CdBD945fE347FDAC4DFA71E13cB3EED7595882
-	BCH: qrrsykuptuu7urt38k4u29j3kvnfa9n3msjssg6cje

If you would like to donate in a currency not listed here, please submit an issue and I will add an address.

Basic Overview and Terminology
------------------------------

The asuran format is split into three logical layers

1.	The Backend/Repository

	Backend and Repository are used somewhat interchangeably. This is the actual place where data gets stored. It is a content addressable storage backend, where blobs (called `Chunk`s) are addressed by an HMAC of their plaintext

2.	The Archive

	This is a data structure, stored in a repository, that describes the way chunks are stitched together to form objects/files. If you think of a repository like a time machine backup, archives are your snapshots

3.	The Manifest

	The manifest is a special data structure stored outside, but adjacent to, the other parts of the repository. It maintains pointers to all the archive structures and provides the root of verification for all the data in the repository.

Comparisons
===========

Comparison with Borg
--------------------

-	Asuran has better use of multithreading

	Asuran has a pipeline that is much better suited to good use of CPU cores, and since archiving speed is typically bound by compression, rather than read/write or encryption/hmac on modern CPUs with an SSD (or just fast spinning rust storage), this leads to a significant speed boost.

-	Switchable storage backends

	Asuran has a framework for describing new storage backends and switching between them at runtime, so you aren't tied to only storing files on a local filesystem or linux machines running SSH.

-	Selectable slicer

	Asuran allows you to chose your content defined chunking algorithm, including the use of FastCDC or a static chunk size slicer, allowing you to decide on your performance/deduplication ratio trade off.

-	Repository format hides chunk length

	Asuran does not suffer from chunk length based fingerprinting attacks like borg does, since we use a repository format that hides chunk length.

Comparison with Restic
----------------------

-	Asuran is much faster

	Asuran is generally faster than borg, and borg is generally faster than Restic, so this one just follows.

-	Optional/Switchable Encryption

	Asuran supports multiple cipher suites of roughly equivalent security, allowing you to meet organizational requirements to use a specific cipher, or to select the cipher that runs fastest on your hardware. You can also completely disable encryption in the event that you actually are backing up to trusted storage.

-	Support for compression

	Asuran lets you chose between no compression, ZStd, LZMA, or LZ4 compression for your repository, allowing you to choose your time/space trade off.

Comparison with Rdedup
----------------------

-	Built in directory traversal

	Asuran will traverse a directory and store its structure automatically. No need to perform an extra step and decrease deduplication by creating a tarball beforehand.

-	Multiple backend support

	See the above note in the borg section

Improvements To All
-------------------

-	High level api suitable for embedding

	Asuran presents a high-level api that is consumed by `asuran-cli`, making it easy to embed support for asuran archives into other applications.

-	Multiple input format support

	Asuran is agnostic to where the files it is backing up are coming from. It doesn't have to be from a filesystem, asuran could just as easily import individual files from a tarball or directly dump tables from a database.

License
-------

Asuran is distributed under the terms of the [BSD 2 Clause + Patent](LICENSE) License.

By contributing to this project, you agree to license your contributions under the terms of the BSD 2 Clause + Patent License.

Contributing
------------

Please see the [contributors guide](CONTRIBUTING.md) for a getting started guide and a primer on our procedures and processes.

If you have any questions, feel free to hop in the chat and ask! We welcome anyone of any skill level.

I am now doing a weekly blog segment on development status [my personal website](https://mccarty.io/). This might be helpful for new contributors to get caught up on what is currently being done.

If you are on github, please hop on over to our gitlab. The github repo is strictly a mirror.

Chat & Support
--------------

Our primary chat is on [Matrix](https://matrix.to/#/!gfTQMJBreSJoPEkEeI:matrix.org?via=matrix.org&via=t2bot.io).

Special Thanks
--------------

This project and its continued development are made possible by our [Hall of Fame](HALL_OF_FAME.md) members.
