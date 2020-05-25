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

A note on stability
-------------------

Asuran and asuran-cli are *pre-alpha software*, prior to version 1.0.0, releases are for evaluation and testing only. Prior to 1.0.0, the API may make breaking changes between patch releases, and there may be breaking format changes between patch releases before 0.2.0 (after 0.2.0, breaking format changes may only happen between minor version increases). Please always read the changelog before updating.

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
