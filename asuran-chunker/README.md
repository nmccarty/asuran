Table of Contents
=================

1.	[Overview](#org57b2eea)
2.	[Features Overview](#org812fd27)
	1.	[Deduplication](#org47fe96a)
	2.	[Encryption](#orga02b764)
		1.	[Key generation and storage](#org42fd216)
	3.	[Compression](#orga3a8341)
		1.	[Intelligent Compression](#org229187f)
	4.	[Authentication](#org31fdd14)
3.	[Development Process / Contributing](#orgae4304f)
	1.	[Roadmap](#org2561a5d)
		1.	[0.1.0](#org2d906fb)
		2.	[0.2.0](#org8db91cd)
4.	[Mission Statement](#org4150617)
	1.	[Suitable for long term archival](#orgb1a085b)
	2.	[Secure](#org72ff4d6)
	3.	[Flexible](#org83eed9c)
	4.	[Fast](#orgcea528a)
	5.	[Easily Embeddable](#org81e70a7)
5.	[Inspiration/Motivation](#orgea481eb)
	1.	[Features Borg has that Restic is missing](#org171cca7)
	2.	[Features Restic has that Borg is missing](#orgca223a8)
	3.	[Features I want that neither has](#org83a4af5)
	4.	[Comparison with rdedup](#org9e9057f)
6.	[Links](#org70a02ca)

<a id="org57b2eea"></a>

Overview
========

Asuran is a new archive format, and an implementation (in rust) of both a read/write library as well as a command line tool.

<a id="org812fd27"></a>

Features Overview
=================

<a id="org47fe96a"></a>

Deduplication
-------------

Deduplication is achieved through a combination of content defined slicing and content addressable storage. The CAS backend will only store each chunk submitted to it once. The content defined slicing provides a reasonable assurance that objects will be broken up into chunks in such a way that duplicate chunks will occur, if possible, and not be stored twice.

<a id="orga02b764"></a>

Encryption
----------

The encryption backend is plug able and can be changed on a chunk-by-chunk basis. Changing the default encryption will only change the encryption method for new chunks.

Each chunk is encrypted and then put in an Object along side a tag indicating the encryption algorithm used, as well as the IV if the algorithm requires one.

<a id="org42fd216"></a>

### Key generation and storage

They keys used by the encryption and HMAC are generated randomly at repository creation, and are stored on disk encrypted with a key derived from a user supplied passphrase. The current KDF used is bcrypt, however this will be swapped out for a more modern one in the near future, most likely argon2id.

<a id="orga3a8341"></a>

Compression
-----------

The compression backend is plugable and supports a variety of compression algorithms. Compression takes place before encryption, and each chunk is tagged with the compression algorithm used to compress it<sup><a id="fnr.1" class="footref" href="#fn.1">1</a></sup>.

<a id="org229187f"></a>

### TODO Intelligent Compression

Asuran will support (but currently does not) intelligent encrypt on, where it will do a trial compression of the first chunk or chunks of a file with lz4, and if reasonable compression is detected, use the selected compression algorithm too compress the chunks of the file, otherwise applying no compression.

<a id="org31fdd14"></a>

Authentication
--------------

All data stored in the repository is authenticated through an encrypt-then-mac construction, with HMAC-SHA256 and Blake2b currently supported. Asuran will sternly refuse to unpack a chunk if the HMAC does not pass verification.

Content keys used by the CAS are generated with an HMAC of the plain text, using a different key than the HMAC used for verification. These are currently not verified.

<a id="orgae4304f"></a>

Development Process / Contributing
==================================

As it is only me developing at the moment, the current development model isn't very structured. In the future it will consist of a branch-per-featured model with branches being required to past a minimum set of tests before being merged into master.

Pull requests and issues are welcome, and by contributing to this project you agree to license your work under the MIT license.

<a id="org2561a5d"></a>

Roadmap
-------

<a id="org2d906fb"></a>

### 0.1.0

Release 0.1.0 should be a somewhat usable product. It will still only operate in append only mode, but will have support for an array of encryption, compression, and HMAC algorithm types. It will additionally have a tentatively stabilized on-disk format. The repository should be able to verify itself as a dedicated operation. The filesystem target should handle sparse data correctly.

1.	TODO libasuran

	libasuran 0.1.0 should have the following features:

	-	[ ] Somewhat stable on disk format
	-	[ ] Support for zlib, lzma, and lz4 compression
	-	[ ] Support for chacha20-poly1305 encryption
	-	[ ] Should have cargo benchmarks
	-	[ ] Should have a working sparse data API
	-	[ ] Should have a method for verifiying the integreity of the repo

2.	TODO asuran

	asuran 0.1.0 should have the following features:

	-	[ ] Support for setting compression type/level
	-	[ ] Support for setting encryption type
	-	[ ] Support for setting HMAC algorithm
	-	[ ] Runtime tests/benchmarks
	-	[ ] Repository verification command

<a id="org8db91cd"></a>

### 0.2.0

<a id="org4150617"></a>

Mission Statement
=================

The asuran archival format is designed to be, in order of importance

<a id="orgb1a085b"></a>

Suitable for long term archival
-------------------------------

Asuran should be a format you should be able to keep your data in forever. Breaking changes to the format (once the release hits 0.1.0) should never lose data in the forward direction, always come with a statically linked binary utility that can convert archives back and forth between the two formats, and always come with through documentation about any structures/features that can not be preserved moving in the backwards direction.

Format versions should be well documented, with easily accessible plaintext documentation, such that a plaintext copy stored alongside an important repository should be sufficient to allow a future engineer to restore the repository without access to an existing asuran implementation.

Long term archival features like optional parity data to guard against bitrot and a built in for in place refreshing by rewriting every segment should be provided.

<a id="org72ff4d6"></a>

Secure
------

Asuran should make good use of encryption and other cryptographic technologies to provide assurance of privacy to the user. Being hostable on untrusted storage, asuran can not hope to completely prevent data tampering, but it should, to the greatest extent possible, be immune to nondestructive tampering (i.e. addition of new files into an archive by an attacker), and be able to detect and reject archives that have been destructively tampered with (i.e. an attacker deleting or modifying files in a repository)

<a id="org83eed9c"></a>

Flexible
--------

Asuran should not place any arbitrary restrictions on the content or structure of data stored in the repository, and should not be limited to the traditional filesystem abstraction. Alternative data layouts, such as photo libraries, email inboxes, and SQL database dumps should enjoy first class citizen status in the Asuran ecosystem.

<a id="orgcea528a"></a>

Fast
----

libasuran should be able to easily saturate a 1Gig ethernet port on a normal consumer grade desktop, or a 10Gig ethernet port on a mid to high tier server, with encryption and a reasonable level of compression turned on. This is assuming that libasuran does not outrun storage of course.

<a id="org81e70a7"></a>

Easily Embeddable
-----------------

The conical Asuran implementation (simply called Asuran) should eat its own dog food by directing all non-trivial repository operations through libasuran. libasuran should expose a well documented and consistent API for interacting with repositories, and should have a well maintained and thoroughly documented C FFI with bindings to, at very least, Python.

<a id="orgea481eb"></a>

Inspiration/Motivation
======================

This project is inspired by both [Borg](https://borgbackup.readthedocs.io/en/stable/) and [Restic](https://restic.net/). Both are very good pieces of software, and perfectly suitable for many use cases, but my use case seems to lie in between the two.

In many ways, this project is intended to be a mashup of what I consider to be the best features of the two applications, while attempting to make a modifiable and extendable framework that can be embedded in other applications easily.

<a id="org171cca7"></a>

Features Borg has that Restic is missing
----------------------------------------

-	Performance Borg generally has way better performance than Restic, in my work load I have personally found this to be to a disturbing extent.
-	Optional/Switchable Encryption Don't get me wrong, being able to safely store sensitive data on untrusted storage is really nice, but sometimes you really are backing up to trusted storage (e.g. an external hard drive that is already encrypted at the file system or drive level), and double encryption is just extra overhead.
-	Optional/Switchable Compression Restic doesn't support compression at all, which, in my opinion, makes it a no-go for many workloads

<a id="orgca223a8"></a>

Features Restic has that Borg is missing
----------------------------------------

-	Switchable Storage Backends This one is a big deal for me. As a home gamer, being able to directly backup my datahoarder levels of files to an unlimited GDrive or the like is a huge deal. This is also the *only* reason I use Restic for some of my backups
-	Multiple computers writing to the same repository Borg's repository locking and chunk cache mechanisms make writing to the same repository with multiple computers a huge pain in the ass. Not having all your computers backing up to the same repository decreases deduplication by an extremely large factor and is just generally not good.

<a id="org83a4af5"></a>

Features I want that neither has
--------------------------------

-	Tar import and export

This isn't entirely true, borg has tar export and is working on tar import, but it lacks one feature that is critical to my workflow, reproducing the same tar file. My work flow involves a program that produces backups as tar files, and when restoring them looks for a special file that must be the first in the tar. I would like the ability to import and export tars and keep the metadata of the tar the same, while still being able to take the tar apart and deduplicate the individual files within it, and use the compression defined by the repository.

-	Good multithreading

	While borg is python based and doesn't really used threads, restic has multithreading, but in my opinion, doesn't use it well

<a id="org9e9057f"></a>

Comparison with [rdedup](https://github.com/dpc/rdedup)
-------------------------------------------------------

rdedup is a very good tool, but falls sort in several areas for me.

-	No built in directory traversal

	rdedup depends on external tools like tar to make backups. In my experience this makes for a poor deduplication rate compared to borg in my workflow.

-	No current support for cloud backends

	This one is almost cheating because asuran does not currently have support for cloud backends, but asuran was designed from the ground up to be storage-agnostic.

-	No intelligent chunking

	rdedup has good support for choosing from a few good content defined slicers, but lacks the framework for intelligent slicing of known data types, such as disk images that can be sliced blockwise, or intelligent picking apart of backups emitted by other applications in a way to maximize deduplication.

-	Little/no integration support

	This complaint also somewhat applies to borg and restic, but to a lesser extent. libasuran is designed to be called into from other applications, such as a carbonite style automatic backup utility, allowing the easy creation of end user friendly applications that support the full suite of asuran features.

<a id="org70a02ca"></a>

Links
=====

-	[Project Website](https://www.asuran.rs/)
-	[Asuran Matrix Chatroom](https://matrix.to/#/!gfTQMJBreSJoPEkEeI:matrix.org?via=matrix.org&via=t2bot.io)

Footnotes
=========

<sup><a id="fn.1" href="#fnr.1">1</a></sup> The compression level used is also included in this tag, regardless of if it is needed or not.
