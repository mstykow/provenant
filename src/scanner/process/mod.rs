// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

mod binary_text;
mod contacts;
mod copyright;
mod license;
mod orchestrator;
mod pipeline;
mod special_cases;
mod spill;

pub use orchestrator::{
    process_collected, process_collected_sequential, process_collected_with_memory_limit,
    process_collected_with_memory_limit_sequential,
};
pub use spill::MemoryMode;
