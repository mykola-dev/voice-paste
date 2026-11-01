// Copyright 2026 The AccessKit Authors. All rights reserved.
// Licensed under the Apache License, Version 2.0 (found in
// the LICENSE-APACHE file) or the MIT license (found in
// the LICENSE-MIT file), at your option.

use std::process::ExitCode;

pub struct Test {
    pub name: &'static str,
    pub run: fn(),
}

macro_rules! tests {
    ($($name:ident),* $(,)?) => {
        &[$($crate::harness::Test { name: stringify!($name), run: $name }),*]
    };
}
pub(crate) use tests;

pub fn run(suites: &[(&str, &[Test])]) -> ExitCode {
    // AppKit requires these regression tests to run on the main thread.
    for (suite, tests) in suites {
        for test in *tests {
            println!("{suite}::{}", test.name);
            (test.run)();
        }
    }
    ExitCode::SUCCESS
}
