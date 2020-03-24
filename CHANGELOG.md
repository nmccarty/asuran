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



