// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

fn main() {
    if let Err(err) = provenant::cli::run() {
        eprintln!("Error: {}", err);
        std::process::exit(1);
    }
}
