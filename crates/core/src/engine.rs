//! 内置象棋引擎：子力 + 位置评估，α-β 搜索 + 吃子静态搜索。
//! 评分单位为"分"（兵 ≈ 60），正数对走棋方有利。

use crate::xiangqi::*;

const MATE: i32 = 1_000_000;
const INF: i32 = 2_000_000;

pub fn piece_value(t: i8) -> i32 {
    match t.abs() {
        K => 100000,
        R => 600,
        H => 270,
        C => 300,
        A => 120,
        E => 120,
        P => 60,
        _ => 0,
    }
}

/// 红方视角的局面静态评估。
pub fn eval(b: &Board) -> i32 {
    let mut score = 0i32;
    for s in 0..90 {
        let p = b.cells[s];
        if p == 0 {
            continue;
        }
        let (r, c) = (row(s), col(s));
        let v = match p.abs() {
            R => 600 + if c == 4 { 6 } else { 0 },
            H => {
                let mut v = 270;
                if (3..=5).contains(&c) {
                    v += 10; // 中路马灵活
                }
                if c == 0 || c == 8 {
                    v -= 8; // 边马受限
                }
                v
            }
            C => 300 + if c == 4 { 10 } else { 0 },
            A | E => 120,
            P => {
                let (adv, crossed) = if p > 0 {
                    (9 - r, r <= 4)
                } else {
                    (r, r >= 5)
                };
                let mut v = 60 + adv as i32 * 6;
                if crossed {
                    v += 30;
                }
                v + (4 - (c as i32 - 4).abs()) * 2
            }
            _ => 0, // 将/帅不计分，胜负由绝杀判定
        };
        score += if p > 0 { v } else { -v };
    }
    score
}

/// 走法排序：吃子优先（MVV-LVA）。
fn order_moves(b: &Board, moves: &mut [Move]) {
    moves.sort_by_key(|mv| {
        let victim = b.cells[mv.to];
        if victim != 0 {
            -(10 * piece_value(victim) - piece_value(b.cells[mv.from]))
        } else {
            0
        }
    });
}

fn negamax(b: &mut Board, depth: i32, mut alpha: i32, beta: i32, ply: i32) -> i32 {
    if depth <= 0 {
        return quiesce(b, alpha, beta, 0);
    }
    let mut moves = b.legal_moves();
    if moves.is_empty() {
        // 无棋可走 = 被绝杀或困毙，走棋方负
        return -MATE + ply;
    }
    order_moves(b, &mut moves);
    let red = b.red_turn;
    let mut best = -INF;
    for mv in moves {
        let mut nb = b.clone();
        nb.make(mv);
        let s = -negamax(&mut nb, depth - 1, -beta, -alpha, ply + 1);
        let _ = red;
        if s > best {
            best = s;
        }
        if s > alpha {
            alpha = s;
        }
        if alpha >= beta {
            break;
        }
    }
    best
}

/// 静态搜索：只延伸吃子，避免"看得见吃子却停手"的地平线效应。
fn quiesce(b: &mut Board, mut alpha: i32, beta: i32, qply: i32) -> i32 {
    let stand = if b.red_turn { eval(b) } else { -eval(b) };
    if stand >= beta {
        return beta;
    }
    if stand > alpha {
        alpha = stand;
    }
    if qply >= 8 {
        return alpha;
    }
    let mut moves: Vec<Move> = b
        .legal_moves()
        .into_iter()
        .filter(|mv| b.cells[mv.to] != 0)
        .collect();
    order_moves(b, &mut moves);
    for mv in moves {
        let mut nb = b.clone();
        nb.make(mv);
        let s = -quiesce(&mut nb, -beta, -alpha, qply + 1);
        if s >= beta {
            return beta;
        }
        if s > alpha {
            alpha = s;
        }
    }
    alpha
}

/// 一条候选走法的分析结果。
#[derive(Clone)]
pub struct Candidate {
    pub mv: Move,
    pub score: i32,     // 走棋方视角
    pub captured: i8,   // 这步吃掉的子（0 表示没吃）
    pub gives_check: bool,
    pub notation: String,
}

pub struct Analysis {
    pub best: Option<Candidate>,
    pub candidates: Vec<Candidate>, // 按分数从高到低
}

/// 从指定深度分析当前局面，返回全部候选走法及评分（供讲解层使用）。
pub fn analyze(b: &Board, depth: i32) -> Analysis {
    let mut moves = b.legal_moves();
    order_moves(b, &mut moves);
    let mut cands = Vec::with_capacity(moves.len());
    let mut alpha = -INF;
    for mv in moves {
        let captured = b.cells[mv.to];
        let gives_check = b.gives_check(mv);
        let notation = b.move_notation(mv);
        let mut nb = b.clone();
        nb.make(mv);
        let score = -negamax(&mut nb, depth - 1, -INF, -alpha, 1);
        cands.push(Candidate {
            mv,
            score,
            captured,
            gives_check,
            notation,
        });
        if score > alpha {
            alpha = score;
        }
    }
    cands.sort_by(|a, b2| b2.score.cmp(&a.score));
    let best = cands.first().cloned();
    Analysis {
        best,
        candidates: cands,
    }
}

/// 只分析指定棋子的所有走法（用于"讲解这枚棋"）。
pub fn analyze_piece(b: &Board, from: usize, depth: i32) -> Analysis {
    let targets = b.legal_targets(from);
    let mut cands = Vec::with_capacity(targets.len());
    for to in targets {
        let mv = Move { from, to };
        let captured = b.cells[to];
        let gives_check = b.gives_check(mv);
        let notation = b.move_notation(mv);
        let mut nb = b.clone();
        nb.make(mv);
        let score = -negamax(&mut nb, depth - 1, -INF, INF, 1);
        cands.push(Candidate {
            mv,
            score,
            captured,
            gives_check,
            notation,
        });
    }
    cands.sort_by(|a, b2| b2.score.cmp(&a.score));
    let best = cands.first().cloned();
    Analysis {
        best,
        candidates: cands,
    }
}

/// 当前硬件/构建模式下合适的搜索深度。
pub fn default_depth() -> i32 {
    if cfg!(debug_assertions) {
        3
    } else {
        4
    }
}
