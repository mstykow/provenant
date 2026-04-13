use super::pipeline::process_file;
use super::special_cases::process_directory;
use super::spill::{FileInfoSpillStore, MemoryMode, retain_or_spill_chunk};
use crate::license_detection::LicenseDetectionEngine;
use crate::models::FileInfo;
use crate::progress::ScanProgress;
use crate::scanner::collect::CollectedPaths;
use crate::scanner::{LicenseScanOptions, ProcessResult, TextDetectionOptions};
use rayon::prelude::*;
use std::sync::Arc;

pub fn process_collected(
    collected: &CollectedPaths,
    progress: Arc<ScanProgress>,
    license_engine: Option<Arc<LicenseDetectionEngine>>,
    license_options: LicenseScanOptions,
    text_options: &TextDetectionOptions,
) -> ProcessResult {
    let mut all_files: Vec<FileInfo> = collected
        .files
        .par_iter()
        .map(|(path, metadata)| {
            let file_entry = process_file(
                path,
                metadata,
                progress.as_ref(),
                license_engine.clone(),
                license_options,
                text_options,
            );
            progress.file_completed(path, metadata.len(), &file_entry.scan_errors);
            file_entry
        })
        .collect();

    for (path, metadata) in &collected.directories {
        all_files.push(process_directory(
            path,
            metadata,
            text_options.collect_info,
            license_engine.is_some(),
        ));
    }

    ProcessResult {
        files: all_files,
        excluded_count: collected.excluded_count,
    }
}

pub fn process_collected_sequential(
    collected: &CollectedPaths,
    progress: Arc<ScanProgress>,
    license_engine: Option<Arc<LicenseDetectionEngine>>,
    license_options: LicenseScanOptions,
    text_options: &TextDetectionOptions,
) -> ProcessResult {
    let mut all_files: Vec<FileInfo> =
        Vec::with_capacity(collected.files.len() + collected.directories.len());

    for (path, metadata) in &collected.files {
        let file_entry = process_file(
            path,
            metadata,
            progress.as_ref(),
            license_engine.clone(),
            license_options,
            text_options,
        );
        progress.file_completed(path, metadata.len(), &file_entry.scan_errors);
        all_files.push(file_entry);
    }

    for (path, metadata) in &collected.directories {
        all_files.push(process_directory(
            path,
            metadata,
            text_options.collect_info,
            license_engine.is_some(),
        ));
    }

    ProcessResult {
        files: all_files,
        excluded_count: collected.excluded_count,
    }
}

pub fn process_collected_with_memory_limit(
    collected: &CollectedPaths,
    progress: Arc<ScanProgress>,
    license_engine: Option<Arc<LicenseDetectionEngine>>,
    license_options: LicenseScanOptions,
    text_options: &TextDetectionOptions,
    max_in_memory: MemoryMode,
) -> ProcessResult {
    let Some((memory_limit, chunk_size)) = memory_limit_settings(max_in_memory) else {
        return process_collected(
            collected,
            progress,
            license_engine,
            license_options,
            text_options,
        );
    };

    let mut retained_files = Vec::new();
    let mut spill_store: Option<FileInfoSpillStore> = None;

    for chunk in collected.files.chunks(chunk_size) {
        let processed_chunk: Vec<FileInfo> = chunk
            .par_iter()
            .map(|(path, metadata)| {
                let file_entry = process_file(
                    path,
                    metadata,
                    progress.as_ref(),
                    license_engine.clone(),
                    license_options,
                    text_options,
                );
                progress.file_completed(path, metadata.len(), &file_entry.scan_errors);
                file_entry
            })
            .collect();

        retain_or_spill_chunk(
            processed_chunk,
            &mut retained_files,
            &mut spill_store,
            memory_limit,
        );
    }

    for (path, metadata) in &collected.directories {
        let entry = process_directory(
            path,
            metadata,
            text_options.collect_info,
            license_engine.is_some(),
        );
        retain_or_spill_chunk(
            vec![entry],
            &mut retained_files,
            &mut spill_store,
            memory_limit,
        );
    }

    if let Some(spill_store) = spill_store {
        retained_files.extend(spill_store.load_all());
    }

    ProcessResult {
        files: retained_files,
        excluded_count: collected.excluded_count,
    }
}

pub fn process_collected_with_memory_limit_sequential(
    collected: &CollectedPaths,
    progress: Arc<ScanProgress>,
    license_engine: Option<Arc<LicenseDetectionEngine>>,
    license_options: LicenseScanOptions,
    text_options: &TextDetectionOptions,
    max_in_memory: MemoryMode,
) -> ProcessResult {
    let Some((memory_limit, chunk_size)) = memory_limit_settings(max_in_memory) else {
        return process_collected_sequential(
            collected,
            progress,
            license_engine,
            license_options,
            text_options,
        );
    };

    let mut retained_files = Vec::new();
    let mut spill_store: Option<FileInfoSpillStore> = None;

    for chunk in collected.files.chunks(chunk_size) {
        let mut processed_chunk: Vec<FileInfo> = Vec::with_capacity(chunk.len());
        for (path, metadata) in chunk {
            let file_entry = process_file(
                path,
                metadata,
                progress.as_ref(),
                license_engine.clone(),
                license_options,
                text_options,
            );
            progress.file_completed(path, metadata.len(), &file_entry.scan_errors);
            processed_chunk.push(file_entry);
        }

        retain_or_spill_chunk(
            processed_chunk,
            &mut retained_files,
            &mut spill_store,
            memory_limit,
        );
    }

    for (path, metadata) in &collected.directories {
        let entry = process_directory(
            path,
            metadata,
            text_options.collect_info,
            license_engine.is_some(),
        );
        retain_or_spill_chunk(
            vec![entry],
            &mut retained_files,
            &mut spill_store,
            memory_limit,
        );
    }

    if let Some(spill_store) = spill_store {
        retained_files.extend(spill_store.load_all());
    }

    ProcessResult {
        files: retained_files,
        excluded_count: collected.excluded_count,
    }
}

fn memory_limit_settings(max_in_memory: MemoryMode) -> Option<(usize, usize)> {
    match max_in_memory {
        MemoryMode::CollectFirst => None,
        MemoryMode::StreamUnlimited => Some((0, 256)),
        MemoryMode::Limit(n) => Some((n, n.max(1))),
    }
}
