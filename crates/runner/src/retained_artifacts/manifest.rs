//! A sealed producer tree, not a discovery list assembled during deletion.
#![allow(unexpected_cfgs)]
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

verus! {
pub enum SealError {
    DirectoryCapacity,
    TreeCapacity,
    Observation(String),
}

impl SealError {
    pub(crate) fn into_message(self) -> (message: String)
        ensures message@ == match self {
            Self::DirectoryCapacity => "directory identity capacity; preserve original"@,
            Self::TreeCapacity => "tree identity manifest capacity; preserve original"@,
            Self::Observation(error) => error@,
        },
    {
        match self {
            Self::DirectoryCapacity => "directory identity capacity; preserve original".to_owned(),
            Self::TreeCapacity => "tree identity manifest capacity; preserve original".to_owned(),
            Self::Observation(error) => error,
        }
    }

    pub(crate) fn permits_overflow(&self, tree: bool) -> (permitted: bool)
        ensures permitted == (tree && matches!(self, Self::DirectoryCapacity | Self::TreeCapacity)),
    {
        tree && matches!(self, Self::DirectoryCapacity | Self::TreeCapacity)
    }
}

/// Last colon strictly before the supplied endpoint, or -1 when absent.
pub open spec fn previous_colon(stamp: Seq<u8>, end: int) -> int
    decreases end,
{
    if end <= 0 || end > stamp.len() { -1 }
    else if stamp[end - 1] == 58 { end - 1 }
    else { previous_colon(stamp, end - 1) }
}

pub open spec fn stamp_data_end(stamp: Seq<u8>) -> int {
    previous_colon(stamp, previous_colon(stamp, stamp.len() as int))
}

fn last_colon(stamp: &[u8], end: usize) -> (found: Option<usize>)
    requires end <= stamp.len(),
    ensures match found {
        Some(i) => i < end && i as int == previous_colon(stamp@, end as int),
        None => previous_colon(stamp@, end as int) == -1,
    },
{
    let mut i = end;
    while i > 0
        invariant i <= end <= stamp.len(),
            previous_colon(stamp@, i as int) == previous_colon(stamp@, end as int),
        decreases i,
    {
        if stamp[i - 1] == 58 { return Some(i - 1); }
        i -= 1;
    }
    None
}

fn data_end(stamp: &[u8]) -> (found: Option<usize>)
    ensures match found {
        Some(i) => i < stamp.len() && i as int == stamp_data_end(stamp@),
        None => stamp_data_end(stamp@) == -1,
    },
{
    match last_colon(stamp, stamp.len()) {
        Some(last) => last_colon(stamp, last),
        None => None,
    }
}

fn same_prefix(left: &[u8], right: &[u8], len: usize) -> (same: bool)
    requires len <= left.len(), len <= right.len(),
    ensures same == (left@.take(len as int) =~= right@.take(len as int)),
{
    let mut i = 0;
    while i < len
        invariant i <= len <= left.len(), len <= right.len(),
            forall|k: int| 0 <= k < i ==> left@[k] == right@[k],
        decreases len - i,
    {
        if left[i] != right[i] {
            assert(left@.take(len as int)[i as int] != right@.take(len as int)[i as int]);
            return false;
        }
        i += 1;
    }
    assert(left@.take(len as int) =~= right@.take(len as int));
    true
}

fn same_bytes(left: &[u8], right: &[u8]) -> (same: bool)
    ensures same == (left@ =~= right@),
{
    if left.len() != right.len() { return false; }
    assert(left@.take(left.len() as int) =~= left@);
    assert(right@.take(left.len() as int) =~= right@);
    same_prefix(left, right, left.len())
}

pub open spec fn regular_file(identity: Seq<u8>) -> bool {
    identity.len() >= 5 && identity[identity.len() - 5] == 58
        && identity[identity.len() - 4] == 102 && identity[identity.len() - 3] == 105
        && identity[identity.len() - 2] == 108 && identity[identity.len() - 1] == 101
}

fn regular_file_identity(identity: &[u8]) -> (file: bool)
    ensures file == regular_file(identity@),
{
    let n = identity.len();
    n >= 5 && identity[n - 5] == 58 && identity[n - 4] == 102
        && identity[n - 3] == 105 && identity[n - 2] == 108 && identity[n - 1] == 101
}

pub open spec fn same_data(left: Seq<u8>, right: Seq<u8>) -> bool {
    let end = stamp_data_end(left);
    end >= 0 && end == stamp_data_end(right) && left.take(end) =~= right.take(end)
}

/// Compare the sealed and observed bytes before any member can be unlinked.
pub fn sealed_member_matches(identity: &[u8], observed_identity: &[u8], directory: bool,
    stamp: &[u8], observed_stamp: &[u8], batch_directory: bool) -> (accepted: bool)
    ensures accepted == (identity@ == observed_identity@ && (directory || stamp@ == observed_stamp@
        || (batch_directory && regular_file(identity@) && same_data(stamp@, observed_stamp@)))),
{
    if !same_bytes(identity, observed_identity) { return false; }
    if directory || same_bytes(stamp, observed_stamp) { return true; }
    if !batch_directory || !regular_file_identity(identity) { return false; }
    match (data_end(stamp), data_end(observed_stamp)) {
        (Some(left), Some(right)) => left == right && same_prefix(stamp, observed_stamp, left),
        _ => false,
    }
}
}

#[cfg(not(verus_keep_ghost))]
impl From<String> for SealError {
    fn from(message: String) -> Self {
        Self::Observation(message)
    }
}

#[cfg(not(verus_keep_ghost))]
impl From<&str> for SealError {
    fn from(message: &str) -> Self {
        Self::Observation(message.to_owned())
    }
}

#[cfg(not(verus_keep_ghost))]
use crate::artifact_disposal::{entry_path, identity};
#[cfg(not(verus_keep_ghost))]
use serde::{Deserialize, Serialize};
#[cfg(not(verus_keep_ghost))]
use std::{fs, path::Path};

#[cfg(not(verus_keep_ghost))]
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Entry {
    pub path: String,
    pub identity: String,
    pub stamp: String,
    pub bytes: u64,
    pub directory: bool,
    #[serde(default)]
    pub allocated_bytes: Option<u64>,
}
#[cfg(not(verus_keep_ghost))]
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Cursor {
    entry: Entry,
    offset: usize,
    children: Option<Vec<String>>,
}
#[cfg(not(verus_keep_ghost))]
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Manifest {
    pub entries: Vec<Entry>,
    pending: Vec<Cursor>,
    pub sealed: bool,
    pub bytes: u64,
}
#[cfg(not(verus_keep_ghost))]
fn stamp(m: &fs::Metadata) -> String {
    use std::os::unix::fs::MetadataExt;
    format!(
        "{}:{}:{}:{}:{}",
        m.len(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec()
    )
}
#[cfg(not(verus_keep_ghost))]
pub(super) fn observe(root: &Path, path: &str) -> Result<Entry, String> {
    let physical = entry_path(root, path)?;
    let m = fs::symlink_metadata(&physical).map_err(|e| e.to_string())?;
    if !m.is_dir() && !m.is_file() && !m.file_type().is_symlink() {
        return Err("special tree entry; preserve original".into());
    }
    Ok(Entry {
        path: path.into(),
        identity: identity(&physical)?,
        stamp: stamp(&m),
        bytes: if m.is_dir() { 0 } else { m.len() },
        directory: m.is_dir(),
        allocated_bytes: {
            use std::os::unix::fs::MetadataExt;
            m.blocks().checked_mul(512)
        },
    })
}
#[cfg(not(verus_keep_ghost))]
impl Manifest {
    /// Each step consumes one declared child after a finite directory snapshot.
    /// Snapshot names and positions survive interruption in owner metadata.
    pub fn step(&mut self, root: &Path, path: &str, limit: usize) -> Result<(), SealError> {
        if self.sealed {
            return Ok(());
        }
        if self.entries.is_empty() {
            let entry = observe(root, path)?;
            self.bytes = entry.bytes;
            self.entries.push(entry.clone());
            if entry.directory {
                self.pending.push(Cursor {
                    entry,
                    offset: 0,
                    children: None,
                });
            } else {
                self.sealed = true;
            }
            return Ok(());
        }
        let Some(cursor) = self.pending.last_mut() else {
            self.sealed = true;
            return Ok(());
        };
        let current = observe(root, &cursor.entry.path)?;
        if current.identity != cursor.entry.identity || current.stamp != cursor.entry.stamp {
            return Err("tree changed while sealing identities; preserve original".into());
        }
        if cursor.children.is_none() {
            // One finite directory snapshot, then one exact declared child per
            // step. Publication charges these names to the global manifest cap.
            // It never opens or follows descendant links.
            let physical = entry_path(root, &cursor.entry.path)?;
            let mut names = Vec::new();
            for item in fs::read_dir(physical).map_err(|e| e.to_string())? {
                if names.len() >= limit {
                    return Err(SealError::DirectoryCapacity);
                }
                let name = item
                    .map_err(|e| e.to_string())?
                    .file_name()
                    .into_string()
                    .map_err(|_| "tree path UTF8")?;
                names.push(name);
            }
            cursor.children = Some(names);
        }
        let child = cursor
            .children
            .as_ref()
            .and_then(|names| names.get(cursor.offset))
            .cloned();
        let next_offset = cursor.offset + 1;
        match child {
            None => {
                self.pending.pop();
                if self.pending.is_empty() {
                    self.sealed = true;
                }
            }
            Some(name) if name == "." || name == ".." => {
                cursor.offset = next_offset;
            }
            Some(name) => {
                if self.entries.len() >= limit {
                    return Err(SealError::TreeCapacity);
                }
                let entry = observe(root, &format!("{}/{name}", cursor.entry.path))?;
                let bytes = self
                    .bytes
                    .checked_add(entry.bytes)
                    .ok_or("tree byte overflow")?;
                self.bytes = bytes;
                cursor.offset = next_offset;
                self.entries.push(entry.clone());
                if entry.directory {
                    self.pending.push(Cursor {
                        entry,
                        offset: 0,
                        children: None,
                    });
                }
            }
        }
        Ok(())
    }
    pub fn validate(entry: &Entry, root: &Path) -> Result<(), String> {
        Self::validate_for_disposal(entry, root, false)
    }

    pub fn validate_for_disposal(
        entry: &Entry,
        root: &Path,
        batch_directory: bool,
    ) -> Result<(), String> {
        let current = observe(root, &entry.path)?;
        // Unlinking a sibling name changes ctime on the same inode. In a
        // disposable batch the producer owns the name, including on interruption.
        // Keep length and mtime checks, and always check the sealed inode and type.
        if !sealed_member_matches(
            entry.identity.as_bytes(),
            current.identity.as_bytes(),
            entry.directory,
            entry.stamp.as_bytes(),
            current.stamp.as_bytes(),
            batch_directory,
        ) {
            return Err("sealed descendant identity/content changed; preserve replacement".into());
        }
        Ok(())
    }
}
