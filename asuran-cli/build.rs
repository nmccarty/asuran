use vergen::{generate_cargo_keys, ConstantsFlags};

fn main() {
    let mut flags = ConstantsFlags::all();
    flags.set(ConstantsFlags::SEMVER, false);
    flags.set(ConstantsFlags::SEMVER_FROM_CARGO_PKG, true);
    generate_cargo_keys(flags).expect("Unable to generate the cargo keys.");
}
