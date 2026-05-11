use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use crate::commit::Commit;
use crate::object_db::{self, IOdb};
use crate::object_id::ObjectId;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot read commit object: {0}")]
    OdbReadFailed(object_db::Error),
}

const FIRST: u32 = 0x1;
const SECOND: u32 = 0x2;
const STALE: u32 = 0x4;
const BASE: u32 = 0x8;

#[derive(Debug)]
struct Elem {
    commit: Commit,
    id: ObjectId,
}

impl Elem {
    fn new(commit: Commit, id: ObjectId) -> Self {
        Self { commit, id }
    }
}

impl Eq for Elem {}

impl PartialEq for Elem {
    fn eq(&self, other: &Self) -> bool {
        self.commit.committer.time.unix_time == other.commit.committer.time.unix_time
    }
}

impl PartialOrd for Elem {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(
            self.commit
                .committer
                .time
                .unix_time
                .cmp(&other.commit.committer.time.unix_time),
        )
    }
}

impl Ord for Elem {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.commit
            .committer
            .time
            .unix_time
            .cmp(&other.commit.committer.time.unix_time)
    }
}

pub fn find_merge_bases(
    odb: &impl IOdb,
    first_id: ObjectId,
    rest: &[ObjectId],
) -> Result<Vec<(Commit, ObjectId)>, Error> {
    if rest.is_empty() {
        return Ok(Vec::new());
    }

    let mut lookup_commit = |id| odb.read_commit(id).map_err(Error::OdbReadFailed);
    if rest.iter().all(|id| *id == first_id) {
        return Ok(vec![(lookup_commit(first_id)?, first_id)]);
    }

    find_best_common_ancestors(&mut lookup_commit, first_id, rest)
}

fn find_best_common_ancestors<F, E>(
    lookup_commit: &mut F,
    first_id: ObjectId,
    rest: &[ObjectId],
) -> Result<Vec<(Commit, ObjectId)>, E>
where
    F: FnMut(ObjectId) -> Result<Commit, E>,
{
    let mut map = HashMap::new();
    let mut queue = BinaryHeap::new();

    queue.push(Elem::new(lookup_commit(first_id)?, first_id));
    map.insert(first_id, FIRST);

    for id in rest {
        queue.push(Elem::new(lookup_commit(*id)?, *id));
        *map.entry(*id).or_insert(0) |= SECOND;
    }

    let mut bases = Vec::new();
    while has_nonstale_elems(&queue, &map) {
        let Elem { commit, id } = queue.pop().unwrap();
        let mut flags = map[&id] & (FIRST | SECOND | STALE);

        if flags == (FIRST | SECOND) {
            if map[&id] & BASE == 0 {
                bases.push((commit.clone(), id));
                *map.get_mut(&id).unwrap() |= BASE;
            }
            flags |= STALE;
        }

        for parent_id in commit.parents {
            let parent_flags = map.entry(parent_id).or_insert(0);
            if *parent_flags & flags == flags {
                continue;
            }

            *map.get_mut(&parent_id).unwrap() |= flags;
            let parent_commit = lookup_commit(parent_id)?;
            queue.push(Elem::new(parent_commit, parent_id));
        }
    }

    let (mut bases, _): (Vec<_>, Vec<_>) =
        bases.into_iter().partition(|(_, id)| map[&id] & STALE == 0);
    bases.sort_by_key(|(commit, _)| Reverse(commit.committer.time.unix_time));
    Ok(bases)
}

fn has_nonstale_elems(queue: &BinaryHeap<Elem>, flags: &HashMap<ObjectId, u32>) -> bool {
    queue
        .iter()
        .find(|elem| flags[&elem.id] & STALE == 0)
        .is_some()
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;
    use std::iter::zip;

    use super::*;
    use crate::signature::{AuthorInfo, CommitterInfo, Signature};
    use crate::testing;
    use crate::time::Time;

    fn get_author_info() -> AuthorInfo {
        let sign =
            Signature::build("user", "example@domain.com", Time::new(1771253662, 10800)).unwrap();
        AuthorInfo(sign)
    }

    fn get_commiter_info() -> CommitterInfo {
        let sign =
            Signature::build("committer", "commiter@email", Time::new(1771253662, 10800)).unwrap();
        CommitterInfo(sign)
    }

    fn get_commit_with_parents(parents: Vec<ObjectId>) -> Commit {
        Commit::new(
            ObjectId::zero(),
            parents,
            get_author_info(),
            get_commiter_info(),
            String::new(),
        )
    }

    #[test]
    fn one_base() -> testing::Result<()> {
        let id = [
            "a123456789012345678901234567890123456789",
            "b123456789012345678901234567890123456789",
            "c123456789012345678901234567890123456789",
        ]
        .map(ObjectId::from_str)
        .map(Result::unwrap);

        let commits = [
            get_commit_with_parents(vec![]),
            get_commit_with_parents(vec![id[0]]),
            get_commit_with_parents(vec![id[0]]),
        ];

        let mut odb = HashMap::new();
        for (id, commit) in zip(&id, &commits) {
            odb.insert(id, commit.clone());
        }

        let actual = find_best_common_ancestors(
            &mut |id| -> Result<Commit, Infallible> { Ok(odb[&id].clone()) },
            id[1],
            &id[2..],
        )?;
        let expected = vec![(commits[0].clone(), id[0])];
        assert_eq!(actual, expected);
        Ok(())
    }

    #[test]
    fn criss_cross() -> testing::Result<()> {
        let id = [
            "a123456789012345678901234567890123456789",
            "b123456789012345678901234567890123456789",
            "c123456789012345678901234567890123456789",
            "d123456789012345678901234567890123456789",
            "e123456789012345678901234567890123456789",
        ]
        .map(ObjectId::from_str)
        .map(Result::unwrap);

        let commits = [
            get_commit_with_parents(vec![]),
            get_commit_with_parents(vec![id[0]]),
            get_commit_with_parents(vec![id[0]]),
            get_commit_with_parents(vec![id[1], id[2]]),
            get_commit_with_parents(vec![id[1], id[2]]),
        ];

        let mut odb = HashMap::new();
        for (id, commit) in zip(&id, &commits) {
            odb.insert(id, commit.clone());
        }

        let actual = find_best_common_ancestors(
            &mut |id| -> Result<Commit, Infallible> { Ok(odb[&id].clone()) },
            id[3],
            &id[4..],
        )?;
        let expected = vec![(commits[1].clone(), id[1]), (commits[2].clone(), id[2])];
        assert_eq!(actual, expected);
        Ok(())
    }

    #[test]
    fn three_commits() -> testing::Result<()> {
        let id = [
            "a123456789012345678901234567890123456789",
            "b123456789012345678901234567890123456789",
            "c123456789012345678901234567890123456789",
            "d123456789012345678901234567890123456789",
            "e123456789012345678901234567890123456789",
        ]
        .map(ObjectId::from_str)
        .map(Result::unwrap);

        let commits = [
            get_commit_with_parents(vec![]),
            get_commit_with_parents(vec![id[0]]),
            get_commit_with_parents(vec![id[1]]),
            get_commit_with_parents(vec![id[1]]),
            get_commit_with_parents(vec![id[0]]),
        ];

        let mut odb = HashMap::new();
        for (id, commit) in zip(&id, &commits) {
            odb.insert(id, commit.clone());
        }

        let actual = find_best_common_ancestors(
            &mut |id| -> Result<Commit, Infallible> { Ok(odb[&id].clone()) },
            id[2],
            &id[3..],
        )?;
        let expected = vec![(commits[1].clone(), id[1])];
        assert_eq!(actual, expected);
        Ok(())
    }
}
