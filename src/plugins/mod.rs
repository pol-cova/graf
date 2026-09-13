// NARROW allow, pending the issue #42 decision (wire up or delete): the
// plugin host has no production caller yet, so its public surface is
// test-only for now. Remove when #42 lands.
#[allow(dead_code)]
pub mod host;
#[allow(dead_code)]
pub mod manifest;
