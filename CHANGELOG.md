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



