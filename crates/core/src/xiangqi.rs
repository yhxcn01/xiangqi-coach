//! 中国象棋核心规则：棋盘表示、走法生成、将军判定、中文记谱法。
//!
//! 棋盘用 90 格数组表示：row 0 是黑方底线（画面上方），row 9 是红方底线（画面下方）。
//! 棋子编码：正数红方、负数黑方；绝对值 1将 2士 3象 4马 5车 6炮 7兵。

pub const K: i8 = 1; // 将/帅
pub const A: i8 = 2; // 士/仕
pub const E: i8 = 3; // 象/相
pub const H: i8 = 4; // 马
pub const R: i8 = 5; // 车
pub const C: i8 = 6; // 炮
pub const P: i8 = 7; // 兵/卒

pub const START_FEN: &str = "rnbakabnr/9/1c5c1/p1p1p1p1p/9/9/P1P1P1P1P/1C5C1/9/RNBAKABNR w";

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Move {
    pub from: usize,
    pub to: usize,
}

#[derive(Clone)]
pub struct Board {
    pub cells: [i8; 90],
    pub red_turn: bool,
}

#[inline]
pub fn row(sq: usize) -> usize {
    sq / 9
}
#[inline]
pub fn col(sq: usize) -> usize {
    sq % 9
}
#[inline]
pub fn sq(r: usize, c: usize) -> usize {
    r * 9 + c
}
#[inline]
pub fn on_board(r: i32, c: i32) -> bool {
    (0..10).contains(&r) && (0..9).contains(&c)
}

impl Board {
    pub fn from_fen(fen: &str) -> Option<Board> {
        let mut cells = [0i8; 90];
        let mut parts = fen.split_whitespace();
        let rows_part = parts.next()?;
        let turn_part = parts.next().unwrap_or("w");
        for (r, row_str) in rows_part.split('/').enumerate() {
            if r >= 10 {
                return None;
            }
            let mut c = 0usize;
            for ch in row_str.chars() {
                if ch.is_ascii_digit() {
                    c += ch.to_digit(10)? as usize;
                } else {
                    if c >= 9 {
                        return None;
                    }
                    let t = match ch.to_ascii_uppercase() {
                        'K' => K,
                        'A' => A,
                        'B' => E,
                        'N' => H,
                        'R' => R,
                        'C' => C,
                        'P' => P,
                        _ => return None,
                    };
                    cells[sq(r, c)] = if ch.is_ascii_uppercase() { t } else { -t };
                    c += 1;
                }
            }
            if c != 9 {
                return None;
            }
        }
        Some(Board {
            cells,
            red_turn: turn_part != "b",
        })
    }

    pub fn start() -> Board {
        Board::from_fen(START_FEN).unwrap()
    }

    pub fn to_fen(&self) -> String {
        let mut s = String::new();
        for r in 0..10 {
            let mut empty = 0;
            for c in 0..9 {
                let p = self.cells[sq(r, c)];
                if p == 0 {
                    empty += 1;
                } else {
                    if empty > 0 {
                        s.push_str(&empty.to_string());
                        empty = 0;
                    }
                    let ch = match p.abs() {
                        K => 'K',
                        A => 'A',
                        E => 'B',
                        H => 'N',
                        R => 'R',
                        C => 'C',
                        _ => 'P',
                    };
                    s.push(if p > 0 { ch } else { ch.to_ascii_lowercase() });
                }
            }
            if empty > 0 {
                s.push_str(&empty.to_string());
            }
            if r < 9 {
                s.push('/');
            }
        }
        s.push_str(if self.red_turn { " w" } else { " b" });
        s
    }

    #[inline]
    pub fn at(&self, r: i32, c: i32) -> i8 {
        if on_board(r, c) {
            self.cells[sq(r as usize, c as usize)]
        } else {
            i8::MAX // 哨兵：界外
        }
    }

    pub fn king_sq(&self, red: bool) -> Option<usize> {
        let target = if red { K } else { -K };
        (0..90).find(|&i| self.cells[i] == target)
    }

    /// 执行走法（就地修改，不翻边），返回被吃子。
    pub fn make(&mut self, mv: Move) -> i8 {
        let cap = self.cells[mv.to];
        self.cells[mv.to] = self.cells[mv.from];
        self.cells[mv.from] = 0;
        self.red_turn = !self.red_turn;
        cap
    }

    /// from 处的棋子是否能攻击到 to（不考虑走后己方被将军，只按走法规则）。
    pub fn attacks(&self, from: usize, to: usize) -> bool {
        let p = self.cells[from];
        if p == 0 {
            return false;
        }
        let red = p > 0;
        let (fr, fc) = (row(from) as i32, col(from) as i32);
        let (tr, tc) = (row(to) as i32, col(to) as i32);
        let dr = tr - fr;
        let dc = tc - fc;
        match p.abs() {
            K => {
                // 宫内一步
                if (dr.abs() + dc.abs()) == 1 && in_palace(tr, tc, red) {
                    return true;
                }
                // 白脸将：同列且中间无子
                if dc == 0 {
                    return self.clear_between(fr, fc, tr, tc) == 0;
                }
                false
            }
            A => dr.abs() == 1 && dc.abs() == 1 && in_palace(tr, tc, red),
            E => {
                dr.abs() == 2
                    && dc.abs() == 2
                    && self.at(fr + dr / 2, fc + dc / 2) == 0
                    && !crosses_river(tr, red)
            }
            H => {
                let leg_ok = if dr.abs() == 2 && dc.abs() == 1 {
                    self.at(fr + dr / 2, fc) == 0
                } else if dr.abs() == 1 && dc.abs() == 2 {
                    self.at(fr, fc + dc / 2) == 0
                } else {
                    return false;
                };
                leg_ok
            }
            R => {
                (dr == 0 || dc == 0) && (dr != 0 || dc != 0) && self.clear_between(fr, fc, tr, tc) == 0
            }
            C => {
                if dr != 0 && dc != 0 || (dr == 0 && dc == 0) {
                    return false;
                }
                self.clear_between(fr, fc, tr, tc) == 1
            }
            P => {
                let fwd = if red { -1 } else { 1 };
                if dr == fwd && dc == 0 {
                    return true;
                }
                let crossed = if red { fr <= 4 } else { fr >= 5 };
                crossed && dr == 0 && dc.abs() == 1
            }
            _ => false,
        }
    }

    /// 直线上 from→to 之间的棋子数（不含端点）。
    fn clear_between(&self, fr: i32, fc: i32, tr: i32, tc: i32) -> i32 {
        let step_r = (tr - fr).signum();
        let step_c = (tc - fc).signum();
        let mut r = fr + step_r;
        let mut c = fc + step_c;
        let mut n = 0;
        while (r, c) != (tr, tc) {
            if self.at(r, c) != 0 {
                n += 1;
            }
            r += step_r;
            c += step_c;
        }
        n
    }

    /// sq 是否被 red=true 的红方 / false 的黑方攻击。
    pub fn is_attacked(&self, target: usize, by_red: bool) -> bool {
        for f in 0..90 {
            let p = self.cells[f];
            if p != 0 && (p > 0) == by_red && self.attacks(f, target) {
                return true;
            }
        }
        false
    }

    /// 某方当前是否被将军。
    pub fn in_check(&self, red: bool) -> bool {
        match self.king_sq(red) {
            Some(k) => self.is_attacked(k, !red),
            None => true, // 老将没了等同于死局
        }
    }

    /// 生成 from 处棋子的伪合法走法。
    fn pseudo_moves_from(&self, from: usize, out: &mut Vec<Move>) {
        let p = self.cells[from];
        if p == 0 {
            return;
        }
        let red = p > 0;
        if red != self.red_turn {
            return;
        }
        let (fr, fc) = (row(from) as i32, col(from) as i32);
        let mut push = |tr: i32, tc: i32| {
            if on_board(tr, tc) {
                let tp = self.at(tr, tc);
                if tp == 0 || (tp > 0) != red {
                    out.push(Move {
                        from,
                        to: sq(tr as usize, tc as usize),
                    });
                }
            }
        };
        match p.abs() {
            K => {
                for (dr, dc) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let (tr, tc) = (fr + dr, fc + dc);
                    if in_palace(tr, tc, red) {
                        push(tr, tc);
                    }
                }
                // 白脸将直接吃对面老将
                for tr in (0..10).map(|x| x as i32) {
                    if tr != fr && self.at(tr, fc).abs() == K && (self.at(tr, fc) > 0) != red {
                        if self.clear_between(fr, fc, tr, fc) == 0 {
                            push(tr, fc);
                        }
                    }
                }
            }
            A => {
                for (dr, dc) in [(1, 1), (1, -1), (-1, 1), (-1, -1)] {
                    let (tr, tc) = (fr + dr, fc + dc);
                    if in_palace(tr, tc, red) {
                        push(tr, tc);
                    }
                }
            }
            E => {
                for (dr, dc) in [(2, 2), (2, -2), (-2, 2), (-2, -2)] {
                    let (tr, tc) = (fr + dr, fc + dc);
                    if on_board(tr, tc)
                        && !crosses_river(tr, red)
                        && self.at(fr + dr / 2, fc + dc / 2) == 0
                    {
                        push(tr, tc);
                    }
                }
            }
            H => {
                const JUMPS: [(i32, i32, i32, i32); 8] = [
                    (2, 1, 1, 0),
                    (2, -1, 1, 0),
                    (-2, 1, -1, 0),
                    (-2, -1, -1, 0),
                    (1, 2, 0, 1),
                    (-1, 2, 0, 1),
                    (1, -2, 0, -1),
                    (-1, -2, 0, -1),
                ];
                for (dr, dc, lr, lc) in JUMPS {
                    if self.at(fr + lr, fc + lc) == 0 {
                        push(fr + dr, fc + dc);
                    }
                }
            }
            R => {
                for (dr, dc) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let mut r = fr + dr;
                    let mut c = fc + dc;
                    while on_board(r, c) {
                        let tp = self.at(r, c);
                        if tp == 0 {
                            push(r, c);
                        } else {
                            if (tp > 0) != red {
                                push(r, c);
                            }
                            break;
                        }
                        r += dr;
                        c += dc;
                    }
                }
            }
            C => {
                for (dr, dc) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let mut r = fr + dr;
                    let mut c = fc + dc;
                    let mut jumped = false;
                    while on_board(r, c) {
                        let tp = self.at(r, c);
                        if !jumped {
                            if tp == 0 {
                                push(r, c);
                            } else {
                                jumped = true;
                            }
                        } else if tp != 0 {
                            if (tp > 0) != red {
                                push(r, c);
                            }
                            break;
                        }
                        r += dr;
                        c += dc;
                    }
                }
            }
            P => {
                let fwd = if red { -1 } else { 1 };
                push(fr + fwd, fc);
                let crossed = if red { fr <= 4 } else { fr >= 5 };
                if crossed {
                    push(fr, fc - 1);
                    push(fr, fc + 1);
                }
            }
            _ => {}
        }
    }

    /// 当前走棋方的全部合法走法。
    pub fn legal_moves(&self) -> Vec<Move> {
        let mut pseudo = Vec::with_capacity(64);
        for f in 0..90 {
            let p = self.cells[f];
            if p != 0 && (p > 0) == self.red_turn {
                self.pseudo_moves_from(f, &mut pseudo);
            }
        }
        let red = self.red_turn;
        pseudo
            .into_iter()
            .filter(|&mv| {
                let mut b = self.clone();
                b.make(mv);
                !b.in_check(red)
            })
            .collect()
    }

    /// from 处棋子的合法目标格。
    pub fn legal_targets(&self, from: usize) -> Vec<usize> {
        let p = self.cells[from];
        if p == 0 || (p > 0) != self.red_turn {
            return vec![];
        }
        let mut pseudo = Vec::new();
        self.pseudo_moves_from(from, &mut pseudo);
        let red = self.red_turn;
        pseudo
            .into_iter()
            .filter(|&mv| {
                let mut b = self.clone();
                b.make(mv);
                !b.in_check(red)
            })
            .map(|mv| mv.to)
            .collect()
    }

    /// 走完 mv 之后，走棋方是否叫将（即对方被将军）。
    pub fn gives_check(&self, mv: Move) -> bool {
        let mut b = self.clone();
        b.make(mv);
        b.in_check(b.red_turn)
    }

    /// 中文记谱法，如「炮二平五」「马八进七」。
    pub fn move_notation(&self, mv: Move) -> String {
        const CN: [&str; 10] = ["", "一", "二", "三", "四", "五", "六", "七", "八", "九"];
        let p = self.cells[mv.from];
        if p == 0 {
            return String::new();
        }
        let red = p > 0;
        let name = piece_char(p);
        let (fr, fc) = (row(mv.from), col(mv.from));
        let (tr, tc) = (row(mv.to), col(mv.to));
        // 红方列号：从右往左 一..九（col8→一，col0→九）；黑方：从左往右 1..9（col0→1）
        let file_no = |c: usize| if red { 9 - c } else { c + 1 };
        let num = |n: usize| -> String {
            if red {
                CN[n].to_string()
            } else {
                n.to_string()
            }
        };
        // 同列同子（前/后）判断
        let same_file: Vec<usize> = (0..10)
            .map(|r| sq(r, fc))
            .filter(|&s| self.cells[s] == p)
            .collect();
        let prefix = if same_file.len() > 1 {
            // 红方行小为前（更靠近对方），黑方行大为前
            let front = if red { fr < row(same_file[if same_file[0] == mv.from { 1 } else { 0 }]) } else { fr > row(same_file[if same_file[0] == mv.from { 1 } else { 0 }]) };
            format!("{}{}", if front { "前" } else { "后" }, name)
        } else {
            format!("{}{}", name, num(file_no(fc)))
        };
        let dir = if tr == fr {
            "平"
        } else {
            let forward = if red { tr < fr } else { tr > fr };
            if forward {
                "进"
            } else {
                "退"
            }
        };
        match p.abs() {
            H | A | E => {
                // 斜走子：方向 + 目标列
                format!("{}{}{}", prefix, dir, num(file_no(tc)))
            }
            _ => {
                if tr == fr {
                    format!("{}平{}", prefix, num(file_no(tc)))
                } else {
                    format!("{}{}{}", prefix, dir, num((tr as i32 - fr as i32).unsigned_abs() as usize))
                }
            }
        }
    }
}

#[inline]
fn in_palace(r: i32, c: i32, red: bool) -> bool {
    if !(3..=5).contains(&c) {
        return false;
    }
    if red {
        (7..=9).contains(&r)
    } else {
        (0..=2).contains(&r)
    }
}

#[inline]
fn crosses_river(r: i32, red: bool) -> bool {
    if red {
        r <= 4
    } else {
        r >= 5
    }
}

/// 棋子对应的中文字（红方：帅仕相马车炮兵；黑方：将士象马砲车卒）。
pub fn piece_char(p: i8) -> &'static str {
    match p {
        K => "帅",
        A => "仕",
        E => "相",
        H => "马",
        R => "车",
        C => "炮",
        P => "兵",
        -1 => "将",
        -2 => "士",
        -3 => "象",
        -4 => "马",
        -5 => "车",
        -6 => "砲",
        -7 => "卒",
        _ => "？",
    }
}

/// 坐标转代数坐标文本（如 r3c4），仅用于调试。
pub fn sq_name(s: usize) -> String {
    format!("r{}c{}", row(s), col(s))
}
