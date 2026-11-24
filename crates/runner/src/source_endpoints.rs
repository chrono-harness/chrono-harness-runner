//! Read both fixed Git endpoints for source-continuity verification.
use chrono_verifier::sources::{SourceVerification, continuity::*};

/// 以下仅为固定 Reader、Serde 和树迭代的接口适配；准入不在这里产生。
pub struct FixedRead<'a> {
    pub reader: &'a crate::facts::Reader,
    pub root: &'a std::path::Path,
    pub base: &'a str,
    pub candidate: &'a str,
}

impl Read for FixedRead<'_> {
    fn blob(&self, base: bool, path: &str) -> Result<Vec<u8>, String> {
        self.reader.blob(
            self.root,
            select_endpoint(base, self.base, self.candidate),
            path,
        )
    }
}

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

#[derive(serde::Deserialize)]
struct DecodedFiles {
    files: Vec<DecodedFile>,
    #[serde(default)]
    reference_syntax: Vec<ReferenceSyntax>,
}
#[derive(serde::Deserialize)]
struct DecodedProjects {
    projects: Vec<ProjectRoot>,
}

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

/// 所有调用方使用此固定两端接口；完整计划与分类库存共用它的同一结果。
pub fn observe(
    reader: &crate::facts::Reader,
    root: &std::path::Path,
    previous: Option<(&str, &serde_json::Value, &serde_json::Value, Vec<String>)>,
    candidate: (&str, &serde_json::Value, &serde_json::Value, Vec<String>),
    paths: &Vec<String>,
) -> Result<Continuity, String> {
    // 空选择没有连续性义务；不为无使用方的证据读取登记或 Git 树。
    if paths.is_empty() {
        return Ok(Continuity::empty());
    }
    let base = previous
        .as_ref()
        .map(|(oid, files, projects, outputs)| {
            observe_endpoint(reader, root, oid, files, projects, outputs.clone())
        })
        .transpose()?;
    let current = observe_endpoint(
        reader,
        root,
        candidate.0,
        candidate.1,
        candidate.2,
        candidate.3,
    )?;
    let io = FixedRead {
        reader,
        root,
        base: previous.map(|row| row.0).unwrap_or(candidate.0),
        candidate: candidate.0,
    };
    if let Some(base) = &base {
        if reader.can_reuse_oid(io.base) && reader.can_reuse_oid(io.candidate) {
            acquire_sources(base, &current, paths, |base, paths| {
                reader
                    .blobs(root, select_endpoint(base, io.base, io.candidate), paths)
                    .map(|_| ())
            })?;
        }
    }
    assess_optional(base.as_ref(), &current, paths, &io)
}
