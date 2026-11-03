// Reused from fixed emela-emb b0a367bc0a29227fae62cee2def75ed925a5218f (Apache-2.0).
#[path = "support/heap.rs"]
mod heap;
#[path = "support/legacy.rs"]
mod legacy;
mod support;
use legacy::{Options, Source, compile_sources};
