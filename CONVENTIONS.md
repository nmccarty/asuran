Commits
=======

This project uses, roughly, the [Conventional Commit](https://www.conventionalcommits.org/en/v1.0.0/) Standard. All commits only affecting one of asuran's crates should use that crates name as the scope. (e.g. a commit affecting the chunker would have the first line of its commit message written as `fix(asuran-chunker): fix a thing`.

The types used in this repository are as follows:

-	feat
-	fix
-	chore
-	ci
-	docs
-	style
-	refactor
-	perf
-	test
-	revert

Commits that fix an issue(s) should have a note about this in their footer, with the format `Fixes
#1, #2`

Imports
=======

Imports in this repository are made in groups, separated by an empty line, in the order Imports from current crate, Imports from other crates in asuran repository, imports from external crates, imports from standard library.

Take this example:

```rust
use crate::repository::backend::common::{IndexTransaction, LockedFile};
use crate::repository::backend::{self, BackendError, Result, SegmentDescriptor};

use asuran_core::repository::ChunkID;

use async_trait::async_trait;
use futures::channel::mpsc;
use futures::channel::oneshot;
use futures::sink::SinkExt;
use futures::stream::StreamExt;
use rmp_serde as rmps;
use tokio::task;

use std::collections::{HashMap, HashSet};
use std::fs::{create_dir, read_dir, File};
use std::io::{BufWriter, Seek, SeekFrom};
use std::path::Path;
```

This is not a hard and fast rule, and there is no deep reasoning behind it, this is just how I like to arrange imports. Your MR isn't going to be rejected if it doesn't follow this format, but please do keep it in mind.
