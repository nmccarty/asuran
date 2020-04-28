Asuran CLI
==========

This is the main cli interface for [asuran](https://gitlab.com/asuran-rs/asuran) ([crates.io](https://crates.io/crates/asuran)), a new, [blazing fast](https://gitlab.com/asuran-rs/archiver-benchmarks/-/blob/master/RESULTS.md) deduplicating archive format, with a zero-compromises security model.

Please see the website at [asuran.rs](https://asuran.rs) for more information, as most of the cool stuff is implemented in the asuran library proper.

Getting Started
---------------

In most cases you will be interacting with the command line asuran clinet (asuran-cli). Either build it from source from the asuran-cli directory in this repository, or install it with:

```bash
env RUSTFLAGS="-C target-feature=+aes,+ssse3" cargo install asuran-cli
```

Optionally add `-C target-cpu=native` for even better performance. The target features (aes and sse3) are required to get good performance, and asuran does not currently offically support being built without them.

This crate is ultimatly an extremely thin wrapper around the asuran API, so most documenation will be found there.

Take a look at the output of `asuran-cli --help` for usage information. Keep in mind that each of the sub-commands has its own help page as well (e.g. `asuran-cli extract --help`).

License
-------

This project is licensed under the BSD 2 Clause + Patent license

Contacting
----------

Join our [matrix chat](https://matrix.to/#/!gfTQMJBreSJoPEkEeI:matrix.org?via=matrix.org&via=t2bot.io) to ask questions, report bugs, or suggest improvements.

Additionally, feel free to open an issue on the gitlab with any bugs you find.
