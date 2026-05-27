use std::{collections::HashMap, ops::Range, usize};

use crate::myers::{Action, Line, Myers, lines};

#[derive(Debug, PartialEq, Eq)]
pub enum Chunk {
    Normal(Vec<Line>),
    Conflict {
        o: Vec<Line>,
        a: Vec<Line>,
        b: Vec<Line>,
    },
}

pub struct Diff3 {
    o: Vec<Line>,
    idx_o: usize,

    a: Vec<Line>,
    idx_a: usize,

    b: Vec<Line>,
    idx_b: usize,

    o_to_a: HashMap<usize, usize>,
    o_to_b: HashMap<usize, usize>,

    chunks: Vec<Chunk>,
}

impl Diff3 {
    pub fn new(original: &[u8], left: &[u8], right: &[u8]) -> Self {
        let o = lines(original);
        let a = lines(left);
        let b = lines(right);
        Self::from_lines(&o, &a, &b)
    }

    pub fn from_lines(original: &[Line], left: &[Line], right: &[Line]) -> Self {
        let o_to_a = equal_lines(original, left);
        let o_to_b = equal_lines(original, right);
        Self {
            o: original.to_vec(),
            a: left.to_vec(),
            b: right.to_vec(),
            idx_o: 0,
            idx_a: 0,
            idx_b: 0,
            chunks: Vec::new(),
            o_to_a,
            o_to_b,
        }
    }

    pub fn compare(mut self) -> Vec<Chunk> {
        while let Some(i) = self.find_next_mismatch() {
            if i == 1 {
                let (o, a, b) = self.find_next_match();
                match (a, b) {
                    (Some(a), Some(b)) => {
                        self.emit_chunk(o, a, b);
                    },
                    _ => {
                        break;
                    },
                }
            } else {
                self.emit_chunk(self.idx_o + i, self.idx_a + i, self.idx_b + i);
            }
        }
        self.emit_final_chunk();
        self.chunks
    }
}

impl Diff3 {
    fn find_next_mismatch(&mut self) -> Option<usize> {
        let mut i = 1;
        while self.is_valid_idx(i)
            && let Some(idx) = self.o_to_a.get(&(self.idx_o + i))
            && *idx == self.idx_a + i
            && let Some(idx) = self.o_to_b.get(&(self.idx_o + i))
            && *idx == self.idx_b + i
        {
            i += 1;
        }

        if self.is_valid_idx(i) { Some(i) } else { None }
    }

    fn find_next_match(&mut self) -> (usize, Option<usize>, Option<usize>) {
        let mut i = self.idx_o + 1;
        while i <= self.o.len() && (!self.o_to_a.contains_key(&i) || !self.o_to_b.contains_key(&i))
        {
            i += 1;
        }
        (
            i,
            self.o_to_a.get(&i).copied(),
            self.o_to_b.get(&i).copied(),
        )
    }

    fn emit_chunk(&mut self, o: usize, a: usize, b: usize) {
        self.add_chunk(self.idx_o..o - 1, self.idx_a..a - 1, self.idx_b..b - 1);
        self.idx_o = o - 1;
        self.idx_a = a - 1;
        self.idx_b = b - 1;
    }

    fn emit_final_chunk(&mut self) {
        self.add_chunk(
            self.idx_o..self.o.len(),
            self.idx_a..self.a.len(),
            self.idx_b..self.b.len(),
        );
    }

    fn add_chunk(&mut self, o: Range<usize>, a: Range<usize>, b: Range<usize>) {
        let o = &self.o[o];
        let a = &self.a[a];
        let b = &self.b[b];
        self.chunks
            .push(create_chunk(o.to_vec(), a.to_vec(), b.to_vec()));
    }

    fn is_valid_idx(&self, i: usize) -> bool {
        self.idx_o + i <= self.o.len()
            || self.idx_a + i <= self.a.len()
            || self.idx_b + i <= self.b.len()
    }
}

fn create_chunk(o: Vec<Line>, a: Vec<Line>, b: Vec<Line>) -> Chunk {
    if o == a && o == b {
        Chunk::Normal(o)
    } else if o == a {
        Chunk::Normal(b)
    } else if o == b {
        Chunk::Normal(a)
    } else {
        Chunk::Conflict { o, a, b }
    }
}

fn equal_lines(a: &[Line], b: &[Line]) -> HashMap<usize, usize> {
    Myers::from_lines(a, b)
        .compare()
        .into_iter()
        .filter(|edit| edit.action == Action::Nothing)
        .map(|edit| {
            let old = edit.a.as_ref().unwrap();
            let new = edit.b.as_ref().unwrap();
            (old.0, new.0)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    fn print_lines(lines: &[Line]) {
        for line in lines {
            let str = std::str::from_utf8(&line.1).unwrap();
            println!("{}", str);
        }
    }

    fn print_chunks(chunks: &[Chunk]) {
        for chunk in chunks {
            match chunk {
                Chunk::Normal(lines) => {
                    print_lines(lines);
                },
                Chunk::Conflict { o, a, b } => {
                    println!("<<<<<<< a");
                    print_lines(a);
                    println!("||||||| o");
                    print_lines(o);
                    println!("=======");
                    print_lines(b);
                    println!(">>>>>>> b");
                },
            }
        }
    }

    #[test]
    fn example() -> testing::Result<()> {
        let o = [
            Line(1, b"a".to_vec()),
            Line(2, b"b".to_vec()),
            Line(3, b"c".to_vec()),
            Line(4, b"d".to_vec()),
            Line(5, b"e".to_vec()),
            Line(6, b"f".to_vec()),
        ];

        let a = [
            Line(1, b"a".to_vec()),
            Line(2, b"d".to_vec()),
            Line(3, b"e".to_vec()),
            Line(4, b"b".to_vec()),
            Line(5, b"c".to_vec()),
            Line(6, b"f".to_vec()),
        ];

        let b = [
            Line(1, b"a".to_vec()),
            Line(2, b"d".to_vec()),
            Line(3, b"b".to_vec()),
            Line(4, b"c".to_vec()),
            Line(5, b"e".to_vec()),
            Line(6, b"f".to_vec()),
        ];

        let diff3 = Diff3::from_lines(&o[..], &a[..], &b[..]);
        let actual = diff3.compare();
        print_chunks(&actual);

        let expected = [
            // from o
            Chunk::Normal(vec![Line(1, b"a".to_vec())]),
            Chunk::Conflict {
                o: vec![
                    Line(2, b"b".to_vec()),
                    Line(3, b"c".to_vec()),
                    Line(4, b"d".to_vec()),
                ],
                a: vec![Line(2, b"d".to_vec())],
                b: vec![
                    Line(2, b"d".to_vec()),
                    Line(3, b"b".to_vec()),
                    Line(4, b"c".to_vec()),
                ],
            },
            Chunk::Normal(vec![Line(3, b"e".to_vec())]),
            Chunk::Normal(vec![Line(4, b"b".to_vec()), Line(5, b"c".to_vec())]),
            Chunk::Normal(vec![Line(6, b"f".to_vec())]),
        ];

        assert_eq!(actual, expected);
        Ok(())
    }
}
