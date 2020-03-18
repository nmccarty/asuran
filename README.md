Asuran
======

We believe that backups should be easy, fast and last forever.

Asuran is a new archive format and rust implementation. It aims to be the archiver for the 2020's, and has been written from the ground up to use the insights from cutting edge research and extract every last bit of performance out of modren hardware, while still providing features users have come to rely on in archivers, like encryption, compression, and global deduplication.

![Codecov](https://img.shields.io/codecov/c/gl/asuran-rs/asuran?style=flat-square) ![Gitlab pipeline status (branch)](https://img.shields.io/gitlab/pipeline/asuran-rs/asuran/master?style=flat-square) ![Crates.io](https://img.shields.io/crates/v/asuran?style=flat-square) ![Crates.io](https://img.shields.io/crates/l/asuran?style=flat-square)

Mission Statement
-----------------

Asuran should be sutible for the long term archival of data, should be operating system and hardware independent, secure, flexible, fast, and easily embeddable.

It should strive to make backups a fast and easy process, and to allow the user to preserve as much of their file history as possible in the space they have avaible. After all, what good is a backup that never ran because it would have taken too long, or the backup that got deleted because it was using too much space?

Asuran should be safe for use on untrusted storage, and should not leak any data that could reveal, to any extent, the contents of the repository.

How Does it Work?
-----------------

Asuran works by splitting your files up into a number of chunks. It splits files up using a selectable Content Defined Chunking algorithim, currently FastCDC by default, so that even if one part of a file changes, it is very likely the other chunks will not..

Those chunks are then (optionally, but on by default) compressed and encrypted, before being comitted to a Content Addressable Storage backend, for later retrievial. Asuran will try its best to only store each chunk it encounters once (and in the most common usecases, it can do this 100% of the time), preventing an archive from storing the same information in the repository more than once if it can avoid it.

The entire archive structure is verified with a Merkele Tree process, so you can be sure that if your restore is successful, then your data is intact and has not been tampered with.

You can store as many archives as you want in a single repository, and write to the same repository with as many computers as you want.

The entire data pipeline is built on top of a modren, async stack, allowing performance that previous contentors in this space could only have dreamed of.

Installing and using
--------------------

In most cases you will be interacting with the command line asuran clinet (asuran-cli). Either build it from source from the asuran-cli directory in this repository, or install it with:

```bash
env RUSTFLAGS="-C target-feature=+aes,+ssse3" cargo install asuran-cli
```

Optionally add `-C target-cpu=native` for even better performance. The target features (aes and sse3) are required to get good performance, and asuran does not currently offically support being built without them.

See `asuran-cli --help` for usage.

`asuran-cli` is, at heart, a thing wrapper that glues togehter the API of the `asuran` library. The `asuran` crate provides a high level interface for interacting with repositories, and will always be a sepereate component and enjoy the same level of support as `asuran-cli` itself.

Documentation
-------------

Please see our [RustDocs](https://asuran-rs.gitlab.io/asuran/asuran/) for api documentation, and the [Internals](https://www.asuran.rs/Internals.html) document for disucssion about the format.

Basic Overview and Terminology
------------------------------

The asuran format is split into three logical layers

1.	The Backend/Repository

	Backend and Repository are used somewhat interchangeably. This is the actual place where data gets stored. It is a content addressable storage backend, where blobs (called `Chunk`s) are addressed by an HMAC of their plaintext

2.	The Archive

	This is a data structure, stored in a repository, that describes the way chunks are stitched together to form objects/files. If you think of a repository like a time machine backup, archives are your snapshots

3.	The Manifest

	The manifest is a special data structure stored outside, but adjacent to, the other parts of the repository. It maintains pointers to all the archive structures and provides the root of verification for all the data in the repository.

License
-------

Asuran is distrubuted under the terms of the [BSD 2 Clause + Patent](LICENSE) License.

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
