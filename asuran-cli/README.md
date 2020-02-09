# Asuran CLI

This is a thin CLI wrapper around [libasuran](https://gitlab.com/asuran-rs/libasuran) ([crates.io](https://crates.io/crates/libasuran))

At the moment this is mostly used for testing and directly tracks the upstream library version.

Please see the website at [asuran.rs](https://asuran.rs) for more information, as most of the cool stuff is implemented in the libasuran library proper.

## Getting Started

```
cargo install asuran
asuran --help
```

## License

This project is licensed under the MIT license

## Limitations

This program is still extremely limited, does not support many operations the backend libary does, and does not make good use of threads, so is much slower than it should be. At the moment it is mostly janked together, and is in need of a complete rewrite dude to a change in upstream philosphy.

While this program mostly works as intended, it has within it the hidden potential to eat your laundry. DO NOT use it as the sole backup for data you care about.

## Contacting

Join our [matrix chat](https://matrix.to/#/!gfTQMJBreSJoPEkEeI:matrix.org?via=matrix.org&via=t2bot.io) or our [Gitter chat](https://gitter.im/Asuran-rs/community?utm_source=share-link&utm_medium=link&utm_campaign=share-link) to ask questions, report bugs, or suggest improvements.

Additionally, feel free to open an issue on the gitlab with any bugs you find. 
