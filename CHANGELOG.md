<a name="0.1.5"></a>
## 0.1.5 (2020-05-31)


#### Bug Fixes

* **asuran-core:**  Fix perf regression with aes feature detection ([840cd30a](https://gitlab.com/asuran-rs/asuran/commit/840cd30a15df0a9d3532535a893f1498e4a9476d))



<a name="0.1.4"></a>
## 0.1.4 (2020-05-30)


#### Breaking Changes

* **asuran:**
  *  Remove name field from StoredArchive ([e5e0cbd6](https://gitlab.com/asuran-rs/asuran/commit/e5e0cbd65267077e835aae345c65d200c85f4f64), closes [#66](https://gitlab.com/asuran-rs/asuran/issues/66), breaks [#](https://gitlab.com/asuran-rs/asuran/issues/))
  *  Replace FlatFile with new format ([fed5466c](https://gitlab.com/asuran-rs/asuran/commit/fed5466c154eb554af460c11a85b1a4fe23c6ab9), breaks [#](https://gitlab.com/asuran-rs/asuran/issues/), [#](https://gitlab.com/asuran-rs/asuran/issues/))
* **asuran-core:**
  *  Remove support for AES CBC ([5ddbc54c](https://gitlab.com/asuran-rs/asuran/commit/5ddbc54c63715d47a5fc146f483228961672cd58), closes [#69](https://gitlab.com/asuran-rs/asuran/issues/69), breaks [#](https://gitlab.com/asuran-rs/asuran/issues/))
  *  Reimplement FlatFile structs for new format ([ff2608f2](https://gitlab.com/asuran-rs/asuran/commit/ff2608f2a00f49d8b608d871c816d4f0261f0a78), breaks [#](https://gitlab.com/asuran-rs/asuran/issues/))

#### Features

*   Expose vendored-openssl feature ([bd7167b9](https://gitlab.com/asuran-rs/asuran/commit/bd7167b9aec0294f07a57aa67b65b2272f62e237))
* **asuran:**
  *  Replace FlatFile with new format ([fed5466c](https://gitlab.com/asuran-rs/asuran/commit/fed5466c154eb554af460c11a85b1a4fe23c6ab9), breaks [#](https://gitlab.com/asuran-rs/asuran/issues/), [#](https://gitlab.com/asuran-rs/asuran/issues/))
  *  Expose cargo feature for SFTP backend ([6c6e1979](https://gitlab.com/asuran-rs/asuran/commit/6c6e19797fdcc512cc2a1f4481ddc4d0e6868a2a))
* **asuran-cli:**  Update asuran-cli to new FlatFile API ([0dac0051](https://gitlab.com/asuran-rs/asuran/commit/0dac0051592a0cbbe4fb8216b4a0d6e2d352e5ad))
* **asuran-core:**
  *  Runtime feature detection for AESNI ([6768925b](https://gitlab.com/asuran-rs/asuran/commit/6768925bcf37b96fd9b857abf9b5695650d7761d), closes [#68](https://gitlab.com/asuran-rs/asuran/issues/68))
  *  Remove support for AES CBC ([5ddbc54c](https://gitlab.com/asuran-rs/asuran/commit/5ddbc54c63715d47a5fc146f483228961672cd58), closes [#69](https://gitlab.com/asuran-rs/asuran/issues/69), breaks [#](https://gitlab.com/asuran-rs/asuran/issues/))
  *  Reimplement FlatFile structs for new format ([ff2608f2](https://gitlab.com/asuran-rs/asuran/commit/ff2608f2a00f49d8b608d871c816d4f0261f0a78), breaks [#](https://gitlab.com/asuran-rs/asuran/issues/))

#### Bug Fixes

* **asuran:**
  *  Remove name field from StoredArchive ([e5e0cbd6](https://gitlab.com/asuran-rs/asuran/commit/e5e0cbd65267077e835aae345c65d200c85f4f64), closes [#66](https://gitlab.com/asuran-rs/asuran/issues/66), breaks [#](https://gitlab.com/asuran-rs/asuran/issues/))
  *  Lock smol at 0.1.4 ([eb24770d](https://gitlab.com/asuran-rs/asuran/commit/eb24770de8093048f06accccb65fb5799f5613c4))
* **asuran-cli:**  Remove AES256CBC ([56efe318](https://gitlab.com/asuran-rs/asuran/commit/56efe31880a73b4bca3c1ae643740201aef97fb8))

#### Other Changes

* **asuran:**
  *  Improve test coverage in sftp module ([520a424b](https://gitlab.com/asuran-rs/asuran/commit/520a424b21c34770c740acb4935ec934dad4efc0))
  *  Add temporary clippy exclusion to backend.rs ([b2445572](https://gitlab.com/asuran-rs/asuran/commit/b2445572df08ebb18a199974bcae049544828c9c))
* **asuran-core:**  Squelch clippy false positive ([3d52ab3a](https://gitlab.com/asuran-rs/asuran/commit/3d52ab3a75506b0c1c4a985cdf0b2ecfdaf971c7))



<a name="0.1.3"></a>
## 0.1.3 (2020-05-15)


#### Features

* **asuran:**
  *  Improve error reporting in sftp backend ([89ccffa2](https://gitlab.com/asuran-rs/asuran/commit/89ccffa269e688c2e2c90861efe59999352f9c6b))
  *  Improve error displays for BackendError ([9b73cb21](https://gitlab.com/asuran-rs/asuran/commit/9b73cb2139dbd9819e4f283c0a1bc6dc81b11f44))
* **asuran-cli:**  Add frontend support for SFTP ([c2533468](https://gitlab.com/asuran-rs/asuran/commit/c25334683b890c4a1219e75966e8cd072f71b2f5))

#### Breaking Changes

* **asuran:**  Make IV regeneration resistant to mishandling ([21fe6987](https://gitlab.com/asuran-rs/asuran/commit/21fe698753c0ab0df7fce31210caeb7c6a9104eb), closes [#65](https://gitlab.com/asuran-rs/asuran/issues/65), breaks [#](https://gitlab.com/asuran-rs/asuran/issues/))

#### Bug Fixes

* **asuran:**  Make IV regeneration resistant to mishandling ([21fe6987](https://gitlab.com/asuran-rs/asuran/commit/21fe698753c0ab0df7fce31210caeb7c6a9104eb), closes [#65](https://gitlab.com/asuran-rs/asuran/issues/65), breaks [#](https://gitlab.com/asuran-rs/asuran/issues/))



<a name="0.1.2"></a>
## 0.1.2 (2020-05-05)


#### Other Changes

*   Add script to run a coverage report ([9c643ccd](https://gitlab.com/asuran-rs/asuran/commit/9c643ccd67350fc4571d92430b7f839ba90593f1))
*   Add scripts to start/stop docker containers ([e99d926b](https://gitlab.com/asuran-rs/asuran/commit/e99d926b1503d1c91e382dd82631e2c1aa14d0eb))
* **asuran:**  Stub and write tests for SFTP Backend ([938b27be](https://gitlab.com/asuran-rs/asuran/commit/938b27bec3c9583b80b8bcfec2303709b74f31f4))
* **asuran-cli:**  Remove DynamicBackend and use object wrappers ([c5997e76](https://gitlab.com/asuran-rs/asuran/commit/c5997e7685ad22bcf0a8fbe0962a1ddce89405d2), closes [#64](https://gitlab.com/asuran-rs/asuran/issues/64))

#### Features

* **asuran:**  Implement SFTP Backend ([67edeaf9](https://gitlab.com/asuran-rs/asuran/commit/67edeaf95753fbecfede57c3ba3d262e686b5263))

#### Bug Fixes

* **asuran:**  Fix incorrect trait implementation for BackendObject ([d1cae196](https://gitlab.com/asuran-rs/asuran/commit/d1cae1960ecb17c496cd1f854ba851b3e43e06ec))
* **asuran-cli:**
  *  Option incorrectly not appearing in subcommand help ([2f096009](https://gitlab.com/asuran-rs/asuran/commit/2f096009509bca977c9e66a9bd076ee410c7eef9))
  *  Fix incorrect output on quiet mode ([6996428e](https://gitlab.com/asuran-rs/asuran/commit/6996428e0ca6a67dfdac2a23296d4af7b773ed9a))
  *  Use smol in multi-threaded mode ([5118afe6](https://gitlab.com/asuran-rs/asuran/commit/5118afe65a1999a22ee5866bb13f8dc914ca4935))



<a name="0.1.1"></a>
## 0.1.1 (2020-04-28)


#### Breaking Changes

* **asuran:**
  *  Replace tokio with smol ([29fbfd63](29fbfd63), closes [#59](59))
  *  Make all queue depths configurable ([9209e0d3](9209e0d3))
  *  Make pipeline API take number of tasks to spawn ([9d566e24](9d566e24))
  *  Remove mostly unused process_id pipeline ([1b123555](1b123555))
  *  Change sync_backend to use dedicated worker thread ([f3fca5b6](f3fca5b6), closes [#62](62))
* **asuran-chunker:**  Make queue depth configurable ([d1e62bb1](d1e62bb1))

#### Performance

* **asuran:**
  *  Replace Archive HashMap with DashMap ([c9a24c70](c9a24c70))
  *  Replace blocking tasks with threads ([64c196f9](64c196f9))
* **asuran-chunker:**  Move chunker work from tasks to dedicated worker threads ([c32ea56f](c32ea56f))

#### Features

* **asuran:**
  *  Make all queue depths configurable ([9209e0d3](9209e0d3))
  *  Make pipeline API take number of tasks to spawn ([9d566e24](9d566e24))
  *  Change sync_backend to use dedicated worker thread ([f3fca5b6](f3fca5b6), closes [#62](62))
  *  Re-export asuran-core features in asuran ([ddf1c734](ddf1c734))
* **asuran-chunker:**  Make queue depth configurable ([d1e62bb1](d1e62bb1))
* **asuran-cli:**
  *  Switch executor over to smol ([56dbdd2d](56dbdd2d))
  *  Configure queue_depths based on pipeline_tasks ([d2790aa1](d2790aa1))
  *  Add pipeline-tasks argument ([745e1538](745e1538))
  *  Reexport asuran features to asuran-cli ([c22f1e0f](c22f1e0f))

#### Other Changes

* **asuran:**
  *  Replace tokio with smol ([29fbfd63](29fbfd63), closes [#59](59))
  *  Replace futures_intrusive with piper ([634cf0f8](634cf0f8))
  *  Add archive NoEncryption / NoCompression Bench ([adf8e286](adf8e286))
  *  Remove mostly unused process_id pipeline ([1b123555](1b123555))
* **asuran-cli:**  Switch cli executor to smol ([7c835e46](7c835e46))

#### Bug Fixes

*   Function call in expect ([79b87773](79b87773))
* **asuran:**  Missing executor feature on futures ([302c73cb](302c73cb))
* **asuran-cli:**  Fix incorrect doc comment on struct opt ([530a0c65](530a0c65))



<a name="0.1.0"></a>
## 0.1.0 (2020-04-23)


#### Features

* **asuran-cli:**  Add quiet flag ([79f4a021](79f4a021))

#### Other Changes

* **asuran:**  Audit use of unwrap ([5a52bd99](5a52bd99))
* **asuran-chunker:**  Audit usage of unwrap ([90d3e2e0](90d3e2e0))
* **asuran-core:**  Audit use of unwrap ([a142aa81](a142aa81))



<a name="0.0.11"></a>
## 0.0.11 (2020-04-20)


#### Other Changes

*   Improve test coverage ([230da7b5](230da7b5))
*   Exclude asuran-cli from cargo tarpaulin ([7679ff91](7679ff91))
* **asuran:**
  *  Improve test coverage for multifile manifest ([924a4027](924a4027))
  *  Create integration test for #56 ([eea0adcf](eea0adcf))
* **asuran-core:**
  *  Remove unused UnpackedChunk type ([423ae195](423ae195))
  *  Add unit tests for listing module ([01303fde](01303fde))

#### Features

*   Eq derives for listing types ([b0e342d2](b0e342d2))
*   Add Hash derive to chunk settings enums ([311a81d7](311a81d7))
*   Restructure StructOpt to include repository options on subcommands ([17123547](17123547))
* **asuran:**
  *  MultiFile backend now creates readlocks ([a49e9bb3](a49e9bb3), closes [#32](32))
  *  make MultiFile backend respects global locks ([caaa3fe7](caaa3fe7))
* **asuran-cli:**
  *  Add preview to asuran-cli extract ([ce544436](ce544436))
  *  Add glob filtering to asuran-cli extract ([553efacb](553efacb), closes [#57](57))
  *  Add contents command ([0752a85a](0752a85a))
* **cli:**
  *  Expand table in bench-crypto to 80 cols ([5254b524](5254b524))
  *  Add bench-crypto command ([37e0c220](37e0c220), closes [#55](55))

#### Bug Fixes

* **asuran-core:**  Correct use of copy_from_slice ([aa267598](aa267598), closes [#56](56))



<a name="0.0.9"></a>
## 0.0.9 (2020-03-24)


#### Other Changes

* **asuran:**
  *  Update benches to use new api ([cbc52bdd](cbc52bdd))
  *  Refactor tests to hit new segment api ([75069bc1](75069bc1))
* **asuran-chunker:**  Replace SmallRng with ChaCha20 and use a precomputed table for buzhash ([4318798a](4318798a), closes [#50](50))

#### Bug Fixes

* **asuran:**  Fix a cache invalidation bug ([803591b4](803591b4), closes [#54](54))

#### Features

* **asuran:**
  *  Implement new segment API ([406380be](406380be))
  *  Implement SegmentDataPart struct ([f40425a6](f40425a6))
  *  Implement SegementHeaderPart struct ([4f02b034](4f02b034))
  *  Add Prelude ([cb8a0c5e](cb8a0c5e), closes [#41](41))
* **chunker:**  Implement static size chunker ([d733e778](d733e778), closes [#3](3))
* **core:**  Add chunk splitting API ([72c30660](72c30660))

#### Breaking Changes

* **asuran-chunker:**  Replace SmallRng with ChaCha20 and use a precomputed table for buzhash ([4318798a](4318798a), closes [#50](50))



<a name="0.0.8"></a>
## 0.0.8 (2020-03-18)


#### Bug Fixes

*   Switch from deyning to warning ([14dfaa43](14dfaa43))
*   Replace buggy roll-my-own semver parser ([de3f3b14](de3f3b14))

#### Other Changes

* **asuran:**  Remove unneded arguments from common Segment API ([c174b2a2](c174b2a2), closes [#47](47))



