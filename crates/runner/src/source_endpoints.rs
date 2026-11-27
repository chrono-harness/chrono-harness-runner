//! Select and read fixed Git endpoints for source-continuity verification.
#[cfg(not(verus_keep_ghost))]
use chrono_verifier::sources::{SourceVerification, continuity::*};
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

verus! {

/// A read operation includes opening the reader, so even that must wait for a selected path.
pub(crate) fn with_selected_paths<T, E, F: FnOnce() -> Result<T, E>>(
    paths: &Vec<String>, empty: T, read: F,
) -> (out: Result<T, E>)
    requires paths.len() != 0 ==> read.requires(()),
    ensures if paths.len() == 0 { out == Ok::<T, E>(empty) } else { read.ensures((), out) },
{
    if paths.is_empty() { Ok(empty) } else { read() }
}

/// Read the original previous endpoint first, when supplied, and keep its first error.
pub(crate) fn read_endpoints<P, T, E, F: Fn(P) -> Result<T, E>>(
    previous: Option<P>, candidate: P, read: F,
) -> (out: Result<(Option<T>, T), E>)
    requires match previous {
        None => read.requires((candidate,)),
        Some(p) => read.requires((p,))
            && forall|value: T| #[trigger] read.ensures((p,), Ok::<T, E>(value))
                ==> read.requires((candidate,)),
    },
    ensures match previous {
        None => match out {
            Ok((base, current)) => base.is_none() && read.ensures((candidate,), Ok::<T, E>(current)),
            Err(error) => read.ensures((candidate,), Err::<T, E>(error)),
        },
        Some(p) => match out {
            Ok((base, current)) => base matches Some(value)
                && read.ensures((p,), Ok::<T, E>(value))
                && read.ensures((candidate,), Ok::<T, E>(current)),
            Err(error) => read.ensures((p,), Err::<T, E>(error))
                || (exists|value: T| #[trigger] read.ensures((p,), Ok::<T, E>(value))
                    && read.ensures((candidate,), Err::<T, E>(error))),
        },
    },
{
    let base = match previous {
        None => None,
        Some(endpoint) => match read(endpoint) {
            Err(error) => return Err(error),
            Ok(observed) => Some(observed),
        },
    };
    match read(candidate) {
        Err(error) => Err(error),
        Ok(current) => Ok((base, current)),
    }
}

/// The candidate reuse check follows a successful previous reuse check.
pub(crate) fn prefetch_if_reusable<'a, T, E, Q, F>(
    previous: Option<(&'a str, T)>, candidate: &'a str, reusable: Q, prefetch: F,
) -> (out: Result<(), E>)
    where Q: Fn(&'a str) -> bool, F: FnOnce(T) -> Result<(), E>,
    requires match previous {
        None => true,
        Some((oid, base)) => reusable.requires((oid,))
            && (reusable.ensures((oid,), true) ==> reusable.requires((candidate,)))
            && (reusable.ensures((oid,), true) && reusable.ensures((candidate,), true)
                ==> prefetch.requires((base,))),
    },
    ensures match previous {
        None => out == Ok::<(), E>(()),
        Some((oid, base)) =>
            ((reusable.ensures((oid,), false)
                || (reusable.ensures((oid,), true) && reusable.ensures((candidate,), false)))
                && out == Ok::<(), E>(()))
            || (reusable.ensures((oid,), true) && reusable.ensures((candidate,), true)
                && prefetch.ensures((base,), out)),
    },
{
    if let Some((oid, base)) = previous {
        if reusable(oid) && reusable(candidate) {
            return prefetch(base);
        }
    }
    Ok(())
}

} // verus!

/// 以下仅为固定 Reader、Serde 和树迭代的接口适配；准入不在这里产生。
#[cfg(not(verus_keep_ghost))]
pub struct FixedRead<'a> {
    pub reader: &'a crate::facts::Reader,
    pub root: &'a std::path::Path,
    pub base: &'a str,
    pub candidate: &'a str,
}

#[cfg(not(verus_keep_ghost))]
impl Read for FixedRead<'_> {
    fn blob(&self, base: bool, path: &str) -> Result<Vec<u8>, String> {
        self.reader.blob(
            self.root,
            select_endpoint(base, self.base, self.candidate),
            path,
        )
    }
}

#[cfg(not(verus_keep_ghost))]
#[derive(serde::Deserialize)]
struct DecodedFile {
    path: String,
    owner: serde_json::Value,
    surface: serde_json::Value,
    cost: serde_json::Value,
    edges: serde_json::Value,
    symlink: Option<serde_json::Value>,
    projection: Option<serde_json::Value>,
    verification: Option<SourceVerification>,
}

#[cfg(not(verus_keep_ghost))]
#[derive(serde::Deserialize)]
struct DecodedFiles {
    files: Vec<DecodedFile>,
    #[serde(default)]
    reference_syntax: Vec<ReferenceSyntax>,
}
#[cfg(not(verus_keep_ghost))]
#[derive(serde::Deserialize)]
struct DecodedProjects {
    projects: Vec<ProjectRoot>,
}

#[cfg(not(verus_keep_ghost))]
pub fn observe_endpoint(
    reader: &crate::facts::Reader,
    root: &std::path::Path,
    oid: &str,
    filemap: &serde_json::Value,
    projects: &serde_json::Value,
    outputs: Vec<String>,
) -> Result<Endpoint, String> {
    let files: DecodedFiles = serde_json::from_value(filemap.clone()).map_err(|e| e.to_string())?;
    let projects: DecodedProjects =
        serde_json::from_value(projects.clone()).map_err(|e| e.to_string())?;
    let rows = files
        .files
        .into_iter()
        .map(|row| {
            Ok(FileObservation {
                path: row.path,
                mode: String::new(),
                kind: String::new(),
                registered: false,
                classified: row.verification.is_some(),
                metadata: serde_json::to_vec(&(
                    row.owner,
                    row.surface,
                    row.cost,
                    row.edges,
                    row.symlink,
                    row.projection,
                ))
                .map_err(|e| e.to_string())?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let tree = reader
        .tree(root, oid)?
        .into_iter()
        .map(|(path, entry)| FileObservation {
            path: path.as_str().to_owned(),
            mode: entry.mode,
            kind: entry.kind,
            registered: false,
            classified: false,
            metadata: Vec::new(),
        })
        .collect();
    Ok(with_reference_syntax(
        assemble(projects.projects, rows, tree, outputs),
        files.reference_syntax,
    ))
}

#[cfg(not(verus_keep_ghost))]
type EndpointInput<'a> = (
    &'a str,
    &'a serde_json::Value,
    &'a serde_json::Value,
    Vec<String>,
);

/// 所有调用方使用此固定两端接口；完整计划与分类库存共用它的同一结果。
#[cfg(not(verus_keep_ghost))]
pub fn observe(
    reader: &crate::facts::Reader,
    root: &std::path::Path,
    previous: Option<EndpointInput<'_>>,
    candidate: EndpointInput<'_>,
    paths: &Vec<String>,
) -> Result<Continuity, String> {
    with_selected_paths(paths, Continuity::empty(), || {
        observe_selected(reader, root, previous, candidate, paths)
    })
}

#[cfg(not(verus_keep_ghost))]
pub(crate) fn observe_selected(
    reader: &crate::facts::Reader,
    root: &std::path::Path,
    previous: Option<EndpointInput<'_>>,
    candidate: EndpointInput<'_>,
    paths: &Vec<String>,
) -> Result<Continuity, String> {
    let candidate_oid = candidate.0;
    let previous_oid = previous.as_ref().map(|endpoint| endpoint.0);
    let (base, current) =
        read_endpoints(previous, candidate, |(oid, files, projects, outputs)| {
            observe_endpoint(reader, root, oid, files, projects, outputs)
        })?;
    let io = FixedRead {
        reader,
        root,
        base: previous_oid.unwrap_or(candidate_oid),
        candidate: candidate_oid,
    };
    prefetch_if_reusable(
        base.as_ref().map(|base| (io.base, base)),
        io.candidate,
        |oid| reader.can_reuse_oid(oid),
        |base| {
            acquire_sources(base, &current, paths, |base, paths| {
                reader
                    .blobs(root, select_endpoint(base, io.base, io.candidate), paths)
                    .map(|_| ())
            })
        },
    )?;
    assess_optional(base.as_ref(), &current, paths, &io)
}
