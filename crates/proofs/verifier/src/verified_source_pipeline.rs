#![allow(unexpected_cfgs)]
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

use super::{ProofInvocation, sources};

verus! {

/// Route planning checks the selected roots only after the source inventory is admitted.
pub fn after_inventory<E, F: FnOnce() -> Result<(), E>>(
    inventory: Result<(), E>, selected: F,
) -> (out: Result<(), E>)
    requires inventory.is_ok() ==> selected.requires(()),
    ensures match inventory {
        Err(error) => out == Err::<(), E>(error),
        Ok(()) => selected.ensures((), out),
    },
{
    match inventory {
        Err(error) => Err(error),
        Ok(()) => selected(),
    }
}

/// The wire decoder supplies the path of each inventory row without copying that row.
pub trait InventoryPath {
    spec fn path_view(&self) -> Seq<char>;

    fn path(&self) -> (out: &str)
        ensures out@ == self.path_view();
}

pub open spec fn selected<T: InventoryPath>(
    files: Seq<T>, nodes: Seq<String>, end: nat,
) -> Seq<Seq<char>>
    decreases end,
{
    if end == 0 || end > files.len() { Seq::empty() }
    else {
        let before = selected(files, nodes, (end - 1) as nat);
        let path = files[end - 1].path_view();
        if exists|j: int| 0 <= j < nodes.len() && (#[trigger] nodes[j])@ == "file:"@ + path {
            before.push(path)
        } else { before }
    }
}

/// Keep inventory order and repeated paths. A disabled inventory requires no Git observation.
pub fn continuity_paths<T: InventoryPath>(
    files: &[T], enabled: Option<bool>, nodes: &Vec<String>,
) -> (out: Vec<String>)
    ensures out@.map_values(|s: String| s@) == if enabled == Some(true) {
        selected(files@, nodes@, files.len() as nat)
    } else { Seq::empty() },
{
    let mut out: Vec<String> = Vec::new();
    if !matches!(enabled, Some(true)) { return out; }
    let mut i = 0;
    while i < files.len()
        invariant i <= files.len(),
            out@.map_values(|s: String| s@) == selected(files@, nodes@, i as nat),
        decreases files.len() - i,
    {
        let path = files[i].path();
        if sources::affected_node("file:", path, nodes) {
            let ghost before = out@.map_values(|s: String| s@);
            out.push(path.to_owned());
            assert(out@.map_values(|s: String| s@) =~= before.push(path@));
        }
        reveal_with_fuel(selected, 2);
        i += 1;
    }
    out
}

pub open spec fn source_pipeline_result<S, P, C, N, T, R, E, O, V>(
    initial: S, previous: P, candidate: C, paths: N, observe: O, validate: V,
    out: (S, Result<R, E>),
) -> bool
    where O: FnOnce(S, P, C, N) -> (S, Result<T, E>),
        V: FnOnce(S, T) -> (S, Result<R, E>),
{
    exists|observed: (S, Result<T, E>)|
        observe.ensures((initial, previous, candidate, paths), observed)
        && match observed.1 {
            Err(error) => out.0 == observed.0 && out.1 == Err::<R, E>(error),
            Ok(evidence) => validate.ensures((observed.0, evidence), out),
        }
}

/// 原两端和所选路径只观察一次；验证只接收该次观察的完整原结果。
pub fn with_source_continuity<S, P, C, N, T, R, E, O, V>(
    state: S, previous: P, candidate: C, paths: N, observe: O, validate: V,
) -> (out: (S, Result<R, E>))
    where O: FnOnce(S, P, C, N) -> (S, Result<T, E>),
        V: FnOnce(S, T) -> (S, Result<R, E>),
    requires forall|s: S, p: P, c: C, n: N| observe.requires((s, p, c, n)),
        forall|s: S, evidence: T| validate.requires((s, evidence)),
    ensures source_pipeline_result(state, previous, candidate, paths, observe, validate, out),
{
    let ghost initial = state;
    let ghost p = previous;
    let ghost c = candidate;
    let ghost n = paths;
    let observed = observe(state, previous, candidate, paths);
    let ghost observation = observed;
    let (state, result) = observed;
    let out = match result {
        Err(error) => (state, Err(error)),
        Ok(evidence) => validate(state, evidence),
    };
    proof {
        assert(observe.ensures((initial, p, c, n), observation));
        assert(exists|observed: (S, Result<T, E>)|
            observe.ensures((initial, p, c, n), observed)
            && match observed.1 {
                Err(error) => out.0 == observed.0 && out.1 == Err::<R, E>(error),
                Ok(evidence) => validate.ensures((observed.0, evidence), out),
            });
    }
    out
}


impl InventoryPath for sources::SourceFile {
    open spec fn path_view(&self) -> Seq<char> { self.path@ }
    fn path(&self) -> (out: &str) { self.path.as_str() }
}

pub open spec fn source_plan_error(
    registry: &sources::SourceRegistry,
    previous: Result<Option<&sources::SourceRegistry>, &str>, nodes: Seq<String>,
    methods: Seq<ProofInvocation>, scopes: Seq<sources::SourceMethodScope>,
    evidence: &sources::continuity::Continuity,
) -> Option<(Seq<char>, Seq<char>)> {
    match sources::registry_classification_error(registry, previous, evidence) {
        Some(error) => Some(error),
        None => sources::selected_plan_error(registry.files.entries@, nodes, methods, scopes,
            matches!(registry.files.require_new_file_classification, Some(true)), evidence, 0),
    }
}

/// 库存先验，原错误优先；两个门使用同一固定库存、影响输入和连续性证据。
pub fn validate_source_plan<'a>(
    registry: &'a sources::SourceRegistry,
    previous: Result<Option<&sources::SourceRegistry>, &'a str>, nodes: &Vec<String>,
    methods: &Vec<ProofInvocation>, scopes: &Vec<sources::SourceMethodScope>,
    evidence: &sources::continuity::Continuity,
) -> (out: Result<(), sources::CoverageError<'a>>)
    ensures match out {
        Ok(()) => source_plan_error(registry, previous, nodes@, methods@, scopes@, evidence).is_none(),
        Err(error) => source_plan_error(registry, previous, nodes@, methods@, scopes@, evidence) == Some(error.view()),
    },
{
    let inventory = sources::validate_file_classifications(registry, previous, evidence);
    after_inventory(inventory, || -> (result: Result<(), sources::CoverageError<'a>>)
        ensures match result {
            Ok(()) => sources::selected_plan_error(registry.files.entries@, nodes@, methods@, scopes@,
                matches!(registry.files.require_new_file_classification, Some(true)), evidence, 0).is_none(),
            Err(error) => sources::selected_plan_error(registry.files.entries@, nodes@, methods@, scopes@,
                matches!(registry.files.require_new_file_classification, Some(true)), evidence, 0) == Some(error.view()),
        },
    { sources::validate_selected_roots(&registry.files, nodes, methods, scopes, evidence) })
}

} // verus!
