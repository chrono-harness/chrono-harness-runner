//! Compare an explicit Git tree/index inventory with literal filesystem objects.
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

verus! {
pub open spec fn hex_ascii(n: int) -> int {
    if n < 10 { 48 + n } else { 87 + n }
}

fn hex_digit(n: u8) -> (digit: u8)
    requires n < 16,
    ensures digit as int == hex_ascii(n as int),
{
    if n < 10 { 48 + n } else { 87 + n }
}

/// Encode each Git digest byte as two lowercase ASCII digits, including leading zeros.
pub fn object_id_ascii(bytes: &[u8]) -> (text: Vec<u8>)
    requires bytes.len() <= usize::MAX / 2,
    ensures text.len() == bytes.len() * 2,
        forall|i: int| 0 <= i < bytes.len() ==>
            text@[2 * i] as int == hex_ascii(#[trigger] bytes@[i] as int / 16)
            && text@[2 * i + 1] as int == hex_ascii(bytes@[i] as int % 16),
{
    let mut text = Vec::with_capacity(bytes.len() * 2);
    let mut i = 0usize;
    while i < bytes.len()
        invariant i <= bytes.len(), bytes.len() <= usize::MAX / 2,
            text.len() == i * 2,
            forall|j: int| 0 <= j < i ==>
                text@[2 * j] as int == hex_ascii(#[trigger] bytes@[j] as int / 16)
                && text@[2 * j + 1] as int == hex_ascii(bytes@[j] as int % 16),
        decreases bytes.len() - i,
    {
        let byte = bytes[i];
        text.push(hex_digit(byte / 16));
        text.push(hex_digit(byte % 16));
        i += 1;
    }
    text
}
}

#[cfg(not(verus_keep_ghost))]
pub(crate) fn object_id_text(bytes: &[u8]) -> String {
    String::from_utf8(object_id_ascii(bytes)).expect("hexadecimal digits are ASCII")
}

#[cfg(not(verus_keep_ghost))]
use crate::{
    facts::{Entry, Tree},
    relative_path,
    verified_identifiers::{git_inventory_records, git_inventory_termination, git_inventory_text},
    verified_status::{
        CheckoutEntryIo, CheckoutEntryMetadata, CheckoutPhysicalObservation,
        checkout_changed_paths, checkout_entry_matches, checkout_hash_stream,
        checkout_index_changes, checkout_object_hash, checkout_physical_path,
        checkout_physical_targets, parse_git_index_rows, parse_git_tree_rows,
        record_checkout_physical_observations,
    },
};
#[cfg(not(verus_keep_ghost))]
use sha1::Sha1;
#[cfg(not(verus_keep_ghost))]
use sha2::Sha256;
#[cfg(not(verus_keep_ghost))]
use std::{
    fs,
    io::{self, Read},
    os::unix::{
        ffi::OsStrExt,
        fs::{OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
};

#[cfg(not(verus_keep_ghost))]
fn rows(raw: &[u8]) -> Result<Vec<&str>, String> {
    git_inventory_termination(raw)?;
    Ok(git_inventory_records(git_inventory_text(
        std::str::from_utf8(raw).ok(),
    )?))
}

#[cfg(not(verus_keep_ghost))]
pub fn tree(raw: &[u8]) -> Result<Tree, String> {
    parse_git_tree_rows(&rows(raw)?)
        .map_err(|error| error.render(|path| format!("invalid relative path: {path:?}")))
}

/// Index stages are compared as data; Git filters/configuration never decide equality.
#[cfg(not(verus_keep_ghost))]
pub fn changes(root: &Path, expected: &Tree, index: &[u8]) -> Result<Vec<String>, String> {
    let inventory = parse_git_index_rows(&rows(index)?)
        .map_err(|error| error.render(|path| format!("invalid relative path: {path:?}")))?;
    let mut changed = checkout_index_changes(expected, inventory.rows.as_slice());
    let targets = checkout_physical_targets(expected, &inventory.stages, &mut changed);
    let observations = targets
        .into_iter()
        .map(|(path, entry)| {
            matches_entry(root, path, entry).map(|matches| CheckoutPhysicalObservation {
                path: path.as_str(),
                matches,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    record_checkout_physical_observations(&mut changed, &observations);
    Ok(checkout_changed_paths(&changed))
}

#[cfg(not(verus_keep_ghost))]
fn physical(root: &Path, path: &str) -> Result<Option<PathBuf>, String> {
    relative_path(path)?;
    checkout_physical_path(
        Path::new(path).components(),
        root.to_path_buf(),
        |mut parts: std::path::Components<'_>| {
            let part = parts.next();
            (parts, part)
        },
        |mut current: PathBuf, part: std::path::Component<'_>| {
            current.push(part);
            current
        },
        |current: PathBuf| {
            let observation = fs::symlink_metadata(&current).map(|metadata| metadata.is_dir());
            (current, observation)
        },
        |error: &io::Error| error.kind() == io::ErrorKind::NotFound,
    )
    .map_err(|error| format!("checkout path {path}: {error}"))
}

#[cfg(not(verus_keep_ghost))]
fn hash<D: Default, O: AsRef<[u8]>>(
    size: u64,
    source: &mut impl Read,
    update: impl Fn(&mut D, &[u8]),
    finalize: impl Fn(D) -> O,
) -> Result<String, String> {
    let mut digest = D::default();
    update(&mut digest, format!("blob {size}\0").as_bytes());
    let mut buffer = [0u8; 65536];
    checkout_hash_stream(
        size,
        source,
        digest,
        &mut buffer,
        |source: &mut _, buffer: &mut [u8; 65536]| {
            let result = Read::read(source, buffer).map_err(|error| error.to_string());
            (source, buffer, result)
        },
        AsRef::<[u8]>::as_ref,
        |mut digest: D, bytes: &[u8]| {
            update(&mut digest, bytes);
            digest
        },
        |digest: D| object_id_text(finalize(digest).as_ref()),
    )
}
#[cfg(not(verus_keep_ghost))]
fn object_hash(oid: &str, size: u64, source: &mut impl Read) -> Result<String, String> {
    checkout_object_hash(
        oid,
        size,
        source,
        |size, source| {
            hash(
                size,
                source,
                |h: &mut Sha1, b| sha1::Digest::update(h, b),
                sha1::Digest::finalize,
            )
        },
        |size, source| {
            hash(
                size,
                source,
                |h: &mut Sha256, b| sha2::Digest::update(h, b),
                sha2::Digest::finalize,
            )
        },
    )
}

#[cfg(not(verus_keep_ghost))]
fn entry_metadata(metadata: fs::Metadata) -> CheckoutEntryMetadata {
    CheckoutEntryMetadata {
        symbolic: metadata.is_symlink(),
        regular: metadata.is_file(),
        size: metadata.len(),
        permissions: metadata.permissions().mode(),
    }
}

#[cfg(not(verus_keep_ghost))]
fn matches_entry(root: &Path, name: &str, entry: &Entry) -> Result<bool, String> {
    checkout_entry_matches(
        entry.kind.as_str(),
        entry.mode.as_str(),
        entry.oid.as_str(),
        CheckoutEntryIo {
            locate: || physical(root, name),
            metadata: |path: &PathBuf| {
                fs::symlink_metadata(path)
                    .map(entry_metadata)
                    .map_err(|error| (error.kind() == io::ErrorKind::NotFound, error))
            },
            format_metadata_error: |error: io::Error| format!("checkout file {name}: {error}"),
            read_link: |path: PathBuf| fs::read_link(path).map_err(|error| error.to_string()),
            hash_link: |target: PathBuf| {
                let bytes = target.as_os_str().as_bytes();
                object_hash(&entry.oid, bytes.len() as u64, &mut &bytes[..])
            },
            open: |path: PathBuf| {
                // 不跟随在类型观察后替入的链接，也不阻塞在替入的 FIFO 上。
                fs::OpenOptions::new()
                    .read(true)
                    .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                    .open(path)
                    .map_err(|error| format!("checkout file {name}: {error}"))
            },
            opened_metadata: |file: &fs::File| {
                file.metadata()
                    .map(entry_metadata)
                    .map_err(|error| error.to_string())
            },
            hash_file: |size: u64, mut file: fs::File| object_hash(&entry.oid, size, &mut file),
            changed_error: || format!("checkout file identity changed while opening: {name}"),
        },
    )
}
