use std::ops::{Index, IndexMut};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line(pub usize, pub Vec<u8>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Nothing,
    Add,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub action: Action,
    pub a: Option<Line>,
    pub b: Option<Line>,
}

impl Edit {
    pub fn nothing(a: Line, b: Line) -> Self {
        Edit {
            action: Action::Nothing,
            a: Some(a),
            b: Some(b),
        }
    }

    pub fn add(line: Line) -> Self {
        Edit {
            action: Action::Add,
            a: None,
            b: Some(line),
        }
    }

    pub fn delete(line: Line) -> Self {
        Edit {
            action: Action::Delete,
            a: Some(line),
            b: None,
        }
    }

    pub fn action(&self) -> Action {
        self.action
    }

    pub fn line(&self) -> &Line {
        debug_assert!(self.a.is_some() && self.b.is_some());
        if let Some(a) = self.a.as_ref() {
            a
        } else if let Some(b) = self.b.as_ref() {
            b
        } else {
            unreachable!()
        }
    }
}

pub fn lines(text: &[u8]) -> Vec<Line> {
    text.split(|byte| *byte == b'\n')
        .enumerate()
        .map(|(i, content)| Line(i + 1, content.to_vec()))
        .collect()
}

#[derive(Debug, Clone, Copy)]
struct Point<T> {
    x: T,
    y: T,
}

impl<T> Point<T> {
    fn new(x: T, y: T) -> Self {
        Point { x, y }
    }
}

#[derive(Debug, Clone, Copy)]
struct Region {
    top_left: Point<isize>,
    bottom_right: Point<isize>,
}

impl Region {
    fn new(left: isize, top: isize, right: isize, bottom: isize) -> Self {
        Self::from_points(Point::new(left, top), Point::new(right, bottom))
    }

    fn from_points(p1: Point<isize>, p2: Point<isize>) -> Self {
        debug_assert!(p1.x <= p2.x, "x1={} x2={}", p1.x, p2.x);
        debug_assert!(p1.y <= p2.y, "y1={} y2={}", p1.y, p2.y);

        Self {
            top_left: p1,
            bottom_right: p2,
        }
    }

    fn width(&self) -> isize {
        self.bottom_right.x - self.top_left.x
    }

    fn height(&self) -> isize {
        self.bottom_right.y - self.top_left.y
    }

    fn size(&self) -> isize {
        self.width() + self.height()
    }

    fn delta(&self) -> isize {
        self.width() - self.height()
    }
}

struct Array {
    d: isize,
    arr: Vec<isize>,
}

impl Array {
    fn new(d: isize) -> Self {
        debug_assert!(d >= 0, "{d} > 0 is false");
        Array {
            d,
            arr: vec![0; 2 * (d as usize) + 1],
        }
    }
}

impl Index<isize> for Array {
    type Output = isize;

    fn index(&self, index: isize) -> &Self::Output {
        debug_assert!(
            (-self.d..=self.d).contains(&index),
            "{} not in [-{}; {}]",
            index,
            self.d,
            self.d
        );
        let idx = (index + self.d) as usize;
        &self.arr[idx]
    }
}

impl IndexMut<isize> for Array {
    fn index_mut(&mut self, index: isize) -> &mut Self::Output {
        // index in [-d; d]
        let idx = (index + self.d) as usize;
        &mut self.arr[idx]
    }
}

pub struct Myers<'a> {
    a: &'a [Line],
    b: &'a [Line],
}

impl<'a> Myers<'a> {
    pub fn from_lines(left: &'a [Line], right: &'a [Line]) -> Self {
        Myers { a: left, b: right }
    }

    pub fn compare(&self) -> Vec<Edit> {
        walk_snakes(self.a, self.b)
            .iter()
            .map(|region| {
                if region.width() == 0 {
                    let line = self.b[region.top_left.y as usize].clone();
                    Edit::add(line)
                } else if region.height() == 0 {
                    let line = self.a[region.top_left.x as usize].clone();
                    Edit::delete(line)
                } else {
                    let left = self.a[region.top_left.x as usize].clone();
                    let right = self.b[region.top_left.y as usize].clone();
                    Edit::nothing(left, right)
                }
            })
            .collect()
    }
}

fn walk_snakes(a: &[Line], b: &[Line]) -> Vec<Region> {
    let mut snakes = Vec::new();
    let path = find_path(Region::new(0, 0, a.len() as isize, b.len() as isize), a, b);
    if path.is_empty() {
        return snakes;
    }

    for p in path.windows(2) {
        let (mut p1, p2) = (p[0], p[1]);

        while p1.x < p2.x && p1.y < p2.y && a[(p1.x) as usize].1 == b[(p1.y) as usize].1 {
            snakes.push(Region::new(p1.x, p1.y, p1.x + 1, p1.y + 1));
            p1.x += 1;
            p1.y += 1;
        }

        if p2.x - p1.x < p2.y - p1.y {
            snakes.push(Region::new(p1.x, p1.y, p1.x, p1.y + 1));
            p1.y += 1;
        } else if p2.x - p1.x > p2.y - p1.y {
            snakes.push(Region::new(p1.x, p1.y, p1.x + 1, p1.y));
            p1.x += 1;
        }

        while p1.x < p2.x && p1.y < p2.y && a[(p1.x) as usize].1 == b[(p1.y) as usize].1 {
            snakes.push(Region::new(p1.x, p1.y, p1.x + 1, p1.y + 1));
            p1.x += 1;
            p1.y += 1;
        }
    }

    snakes
}

fn find_path(region: Region, a: &[Line], b: &[Line]) -> Vec<Point<isize>> {
    let mut path = Vec::new();
    if let Some(snake) = midpoint(region, a, b) {
        let left = Region::from_points(region.top_left, snake.top_left);
        let right = Region::from_points(snake.bottom_right, region.bottom_right);
        let mut head = find_path(left, a, b);
        let mut tail = find_path(right, a, b);
        if head.is_empty() {
            path.push(snake.top_left);
        } else {
            path.append(&mut head);
        }

        if tail.is_empty() {
            path.push(snake.bottom_right);
        } else {
            path.append(&mut tail);
        }
    }
    path
}

fn midpoint(region: Region, a: &[Line], b: &[Line]) -> Option<Region> {
    if region.size() == 0 {
        return None;
    }

    let mut max = region.size() / 2;
    if region.size() % 2 != 0 {
        max += 1;
    }

    let mut forward = Array::new(max);
    forward[1] = region.top_left.x;

    let mut backward = Array::new(max);
    backward[1] = region.bottom_right.y;

    for d in 0..=max {
        if let Some(snake) = try_forward_move(region, &mut forward, &backward, d, a, b) {
            return Some(snake);
        }

        if let Some(snake) = try_backward_move(region, &forward, &mut backward, d, a, b) {
            return Some(snake);
        }
    }
    unreachable!();
}

fn try_forward_move(
    region: Region,
    forward: &mut Array,
    backward: &Array,
    d: isize,
    a: &[Line],
    b: &[Line],
) -> Option<Region> {
    for k in (-d..=d).rev().step_by(2) {
        let c = k - region.delta();

        let (px, mut x) = if k == -d || (k != d && forward[k - 1] < forward[k + 1]) {
            (forward[k + 1], forward[k + 1])
        } else {
            (forward[k - 1], forward[k - 1] + 1)
        };

        let mut y = region.top_left.y + (x - region.top_left.x) - k;
        let py = if d == 0 || px != x { y } else { y - 1 };

        while x < region.bottom_right.x
            && y < region.bottom_right.y
            && a[x as usize].1 == b[y as usize].1
        {
            x += 1;
            y += 1;
        }

        forward[k] = x;

        if region.delta() % 2 != 0 && (-(d - 1)..=(d - 1)).contains(&c) && y >= backward[c] {
            return Some(Region::new(px, py, x, y));
        }
    }
    None
}

fn try_backward_move(
    region: Region,
    forward: &Array,
    backward: &mut Array,
    d: isize,
    a: &[Line],
    b: &[Line],
) -> Option<Region> {
    for c in (-d..=d).rev().step_by(2) {
        let k = c + region.delta();

        let (py, mut y) = if c == -d || (c != d && backward[c - 1] > backward[c + 1]) {
            (backward[c + 1], backward[c + 1])
        } else {
            (backward[c - 1], backward[c - 1] - 1)
        };

        let mut x = region.top_left.x + (y - region.top_left.y) + k;
        let px = if d == 0 || py != y { x } else { x + 1 };

        while x > region.top_left.x
            && y > region.top_left.y
            && a[(x - 1) as usize].1 == b[(y - 1) as usize].1
        {
            x -= 1;
            y -= 1;
        }

        backward[c] = y;

        if region.delta() % 2 == 0 && (-d..=d).contains(&k) && x <= forward[k] {
            return Some(Region::new(x, y, px, py));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    #[test]
    fn example() -> testing::Result<()> {
        let a = b"A\nB\nC\nA\nB\nB\nA";
        let a = lines(a);

        let b = b"C\nB\nA\nB\nA\nC";
        let b = lines(b);

        let alg = Myers::from_lines(&a, &b);
        let actual = alg.compare();

        let expected = [
            Edit::delete(Line(1, b"A".to_vec())),
            Edit::delete(Line(2, b"B".to_vec())),
            Edit::nothing(Line(3, b"C".to_vec()), Line(1, b"C".to_vec())),
            Edit::delete(Line(4, b"A".to_vec())),
            Edit::nothing(Line(5, b"B".to_vec()), Line(2, b"B".to_vec())),
            Edit::add(Line(3, b"A".to_vec())),
            Edit::nothing(Line(6, b"B".to_vec()), Line(4, b"B".to_vec())),
            Edit::nothing(Line(7, b"A".to_vec()), Line(5, b"A".to_vec())),
            Edit::add(Line(6, b"C".to_vec())),
        ];

        assert_eq!(actual, expected);
        Ok(())
    }
}
