//! 浅历史文件读取的状态、回复和观察次序。
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

verus! {

/// Map one file read to the public status and its exact reply. The OS adapter
/// supplies the NotFound observation and formats the original diagnostic.
pub fn shallow_file_reply<T,E>(read: Result<T,(bool,E)>, empty: T)
    -> (out: (&'static str, Result<T,E>))
    ensures match read {
        Ok(rows) => out == ("present", Ok::<T,E>(rows)),
        Err((true, _)) => out == ("absent", Ok::<T,E>(empty)),
        Err((false, error)) => out == ("error", Err::<T,E>(error)),
    },
{
    match read {
        Ok(rows) => ("present", Ok(rows)),
        Err((true, _)) => ("absent", Ok(empty)),
        Err((false, error)) => ("error", Err(error)),
    }
}

pub fn retain_shallow_read<T, F: FnOnce() -> T>(records: &mut Vec<T>, bound: bool, record: F)
    requires bound ==> record.requires(()),
    ensures
        !bound ==> records@ == old(records)@,
        bound ==> records@.len() == old(records)@.len() + 1,
        bound ==> records@.drop_last() == old(records)@,
        bound ==> record.ensures((), records@.last()),
{
    if bound {
        records.push(record());
    }
}

pub fn observed_shallow_reads<T>(records: &Vec<T>) -> (out: Option<&Vec<T>>)
    ensures out == if records@.len() == 0 { None } else { Some(records) },
{
    if records.is_empty() { None } else { Some(records) }
}

}
