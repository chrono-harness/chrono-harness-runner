//! Admissibility and the stable Kahn walk used by full and inventory.
//! Equal UTF-8 identifiers have equal ranks. Smaller ranks precede larger ones.
use super::identifiers::equal_text;
use std::collections::{BTreeMap, BTreeSet};
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

verus! {

pub struct Declaration<'a> {
    pub id: usize,
    pub empty: bool,
    pub selector: &'a str,
    pub modes: &'a [String],
    pub after: Vec<usize>,
}

#[cfg_attr(not(verus_keep_ghost), derive(Debug, PartialEq))]
pub enum Error { Id, Transport, DuplicatePredecessor, Empty, Missing(usize, usize), Cycle }

fn unique(ids: &[usize]) -> (out: bool)
    ensures out == ids@.no_duplicates(),
{
    let mut seen = BTreeSet::new();
    let mut i = 0;
    while i < ids.len()
        invariant i <= ids.len(), seen@ == ids@.take(i as int).to_set(),
            ids@.take(i as int).no_duplicates(),
        decreases ids.len() - i,
    {
        let ghost prefix = ids@.take(i as int);
        if !seen.insert(ids[i]) {
            proof {
                let j = choose|j: int| 0 <= j < prefix.len() && prefix[j] == ids@[i as int];
                assert(ids@[j] == ids@[i as int]);
            }
            return false;
        }
        proof {
            assert(ids@.take(i + 1) =~= prefix.push(ids@[i as int]));
            prefix.lemma_push_to_set_commute(ids@[i as int]);
        }
        i += 1;
    }
    assert(ids@.take(ids.len() as int) =~= ids@);
    true
}

pub open spec fn entry_error(row: &Declaration, seen: Set<usize>, selector: &str, mode: &str)
    -> Option<Error>
{
    if row.empty || seen.contains(row.id) { Some(Error::Id) }
    else if row.selector@ != selector@ || row.modes.len() != 1 || row.modes@[0]@ != mode@ {
        Some(Error::Transport)
    } else if !row.after@.no_duplicates() { Some(Error::DuplicatePredecessor) }
    else { None }
}

pub fn check_entry(row: &Declaration, seen: &BTreeMap<usize, usize>, selector: &str, mode: &str)
    -> (out: Option<Error>)
    ensures out == entry_error(row, seen@.dom(), selector, mode),
{
    if row.empty || seen.contains_key(&row.id) { return Some(Error::Id); }
    if !equal_text(row.selector, selector) || row.modes.len() != 1
        || !equal_text(row.modes[0].as_str(), mode) {
        return Some(Error::Transport);
    }
    if !unique(&row.after) { return Some(Error::DuplicatePredecessor); }
    None
}

/// The same ordered predecessor lookup identifies a missing declaration before the walk
/// and an outstanding dependency during the walk.
pub fn first_parent(after: &[usize], index: &BTreeMap<usize, usize>, present: bool)
    -> (out: Option<usize>)
    ensures match out {
        Some(i) => i < after.len() && index@.contains_key(after@[i as int]) == present
            && forall|j: int| 0 <= j < i ==> index@.contains_key(#[trigger] after@[j]) != present,
        None => forall|j: int| 0 <= j < after.len() ==> index@.contains_key(#[trigger] after@[j]) != present,
    },
{
    let mut i = 0;
    while i < after.len()
        invariant i <= after.len(),
            forall|j: int| 0 <= j < i ==> index@.contains_key(#[trigger] after@[j]) != present,
        decreases after.len() - i,
    {
        if index.contains_key(&after[i]) == present { return Some(i); }
        i += 1;
    }
    None
}

pub open spec fn indexed(rows: Seq<Declaration>, pending: Map<usize, usize>) -> bool {
    forall|id: usize| pending.contains_key(id) ==> pending[id] < rows.len()
        && rows[pending[id] as int].id == id
}

pub open spec fn ready(rows: Seq<Declaration>, pending: Map<usize, usize>, id: usize) -> bool {
    pending.contains_key(id) && {
        let after = rows[pending[id] as int].after@;
        forall|j: int| 0 <= j < after.len() ==> !pending.contains_key(#[trigger] after[j])
    }
}

pub open spec fn smallest(rows: Seq<Declaration>, pending: Map<usize, usize>, id: usize) -> bool {
    ready(rows, pending, id) && forall|other: usize| ready(rows, pending, other) ==> id <= other
}

pub fn next_ready(rows: &[Declaration], pending: &BTreeMap<usize, usize>) -> (out: Option<usize>)
    requires indexed(rows@, pending@),
    ensures match out {
        Some(id) => smallest(rows@, pending@, id),
        None => forall|id: usize| !ready(rows@, pending@, id),
    },
{
    let mut keys = pending.keys();
    let ghost all = keys@.1;
    proof {
        vstd::std_specs::btree::axiom_increasing_seq_meaning(all);
        assert forall|i: int, j: int| 0 <= i < j < all.len() implies (#[trigger] all[i]) < (#[trigger] all[j]) by {
            assert(vstd::std_specs::cmp::OrdSpec::cmp_spec(&all[i], &all[j]) is Less);
        }
    }
    loop
        invariant keys@.1 == all, 0 <= keys@.0 <= all.len(),
            all.to_set() == pending@.dom(), indexed(rows@, pending@),
            forall|i: int, j: int| 0 <= i < j < all.len() ==> (#[trigger] all[i]) < (#[trigger] all[j]),
            forall|i: int| 0 <= i < keys@.0 ==> !ready(rows@, pending@, #[trigger] all[i]),
        decreases all.len() - keys@.0,
    {
        let ghost pos = keys@.0;
        match keys.next() {
            Some(id) => {
                assert(all.contains(*id));
                let i = *pending.get(id).unwrap();
                if first_parent(&rows[i].after, pending, true).is_none() {
                    assert forall|other: usize| ready(rows@, pending@, other) implies *id <= other by {
                        let j = choose|j: int| 0 <= j < all.len() && all[j] == other;
                        assert(all[pos] == *id);
                    }
                    return Some(*id);
                }
            },
            None => {
                assert forall|id: usize| !ready(rows@, pending@, id) by {
                    if ready(rows@, pending@, id) {
                        let j = choose|j: int| 0 <= j < all.len() && all[j] == id;
                        assert(!ready(rows@, pending@, all[j]));
                    }
                }
                return None;
            },
        }
    }
}

pub open spec fn walked(rows: Seq<Declaration>, initial: Map<usize, usize>, order: Seq<usize>,
    remaining: Map<usize, usize>) -> bool
    decreases order.len(),
{
    if order.len() == 0 { remaining == initial } else {
        let ix = order.last();
        let id = rows[ix as int].id;
        let before = remaining.insert(id, ix);
        ix < rows.len() && !remaining.contains_key(id) && smallest(rows, before, id)
            && walked(rows, initial, order.drop_last(), before)
    }
}

pub open spec fn stalled(rows: Seq<Declaration>, initial: Map<usize, usize>) -> bool {
    exists|prefix: Seq<usize>, remaining: Map<usize, usize>|
        walked(rows, initial, prefix, remaining) && prefix.len() + remaining.len() == initial.len()
            && remaining.len() > 0
            && (forall|id: usize| !ready(rows, remaining, id))
}

pub open spec fn walk_result(rows: Seq<Declaration>, initial: Map<usize, usize>,
    out: Result<Vec<usize>, Error>) -> bool {
    match out {
        Ok(order) => order.len() == initial.len() && walked(rows, initial, order@, Map::empty()),
        Err(e) => e == Error::Cycle && stalled(rows, initial),
    }
}

pub fn walk(rows: &[Declaration], initial_index: BTreeMap<usize, usize>)
    -> (out: Result<Vec<usize>, Error>)
    requires indexed(rows@, initial_index@), initial_index@.len() <= rows.len(),
    ensures walk_result(rows@, initial_index@, out),
{
    let ghost initial = initial_index@;
    let mut pending = initial_index;
    let mut order = Vec::new();
    while !pending.is_empty()
        invariant initial == initial_index@, indexed(rows@, pending@), order.len() + pending@.len() == initial.len(),
            walked(rows@, initial, order@, pending@),
        decreases pending@.len(),
    {
        let ghost before = pending@;
        let ghost prefix = order@;
        let result = next_ready(rows, &pending);
        if result.is_none() {
            assert(stalled(rows@, initial)) by {
                assert(pending@.len() > 0);
                assert(walked(rows@, initial, order@, pending@));
            }
            assert(initial == initial_index@);
            assert(walk_result(rows@, initial_index@, Err(Error::Cycle)));
            return Err(Error::Cycle);
        }
        let id = result.unwrap();
        let i = pending.remove(&id).unwrap();
        order.push(i);
        proof {
            assert(pending@.insert(id, i) =~= before);
            assert(order@.drop_last() =~= prefix);
            assert(walked(rows@, initial, prefix, before));
            assert(walked(rows@, initial, order@, pending@));
        }
    }
    assert(pending@ =~= Map::empty());
    Ok(order)
}

pub open spec fn declaration_index(rows: Seq<Declaration>, count: int) -> Map<usize, usize>
    decreases count,
{
    if count <= 0 { Map::empty() }
    else { declaration_index(rows, count - 1).insert(rows[count - 1].id, (count - 1) as usize) }
}

pub open spec fn admitted(rows: Seq<Declaration>, count: int, selector: &str, mode: &str) -> bool {
    forall|i: int| 0 <= i < count ==> entry_error(#[trigger] &rows[i],
        declaration_index(rows, i).dom(), selector, mode).is_none()
}

pub open spec fn referenced(rows: Seq<Declaration>, index: Map<usize, usize>, count: int) -> bool {
    forall|i: int, j: int| 0 <= i < count && 0 <= j < rows[i].after.len()
        ==> index.contains_key(#[trigger] rows[i].after@[j])
}

pub open spec fn schedule_result(rows: Seq<Declaration>, selector: &str, mode: &str,
    out: Result<Vec<usize>, Error>) -> bool {
    let index = declaration_index(rows, rows.len() as int);
    match out {
        Err(Error::Id) | Err(Error::Transport) | Err(Error::DuplicatePredecessor) =>
            exists|i: int| 0 <= i < rows.len() && admitted(rows, i, selector, mode)
                && entry_error(&rows[i], declaration_index(rows, i).dom(), selector, mode) == Some(out->Err_0),
        _ => admitted(rows, rows.len() as int, selector, mode) && match out {
            Err(Error::Empty) => rows.len() == 0,
            Err(Error::Missing(i, j)) => i < rows.len() && j < rows[i as int].after.len()
                && !index.contains_key(rows[i as int].after@[j as int]) && referenced(rows, index, i as int)
                && forall|k: int| 0 <= k < j ==> index.contains_key(#[trigger] rows[i as int].after@[k]),
            _ => rows.len() > 0 && referenced(rows, index, rows.len() as int)
                && walk_result(rows, index, out),
        },
    }
}

} // verus!

#[cfg(verus_keep_ghost)]
pub use super::full::order_declarations as schedule;
#[cfg(not(verus_keep_ghost))]
pub use crate::full::order_declarations as schedule;
