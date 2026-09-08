//! AI 讲解文案层（纯计算，服务端与 WASM 共用）。
//! 引擎给出确定性结论（最佳走法 + 全部候选评分），本模块把它组织成
//! 喂给 LLM 的提示词，以及未配置 AI 时降级用的模板化基础提示——宁可不讲，也不乱讲。

use crate::engine::{Analysis, Candidate};
use crate::xiangqi::*;

pub const SYSTEM_PROMPT: &str = "你是一位耐心的中国象棋教练，用大白话给业余爱好者讲棋。\
规则：内置引擎已经算出确定性结论，你只负责解释，绝不能推翻引擎结论，也不能另行推荐其他走法。\
讲解 120~220 字，口语化，分点讲清楚：对方这步棋想干什么、你为什么该走引擎推荐的那步、有哪些看着正常其实是臭棋的走法要避开。\
评分单位是分（一个兵约 60 分），正数对走棋方有利。着法用中文记谱法（如炮二平五）。";

pub fn describe_candidate(c: &Candidate) -> String {
    let mut tags = Vec::new();
    if c.captured != 0 {
        tags.push(format!("吃掉{}", piece_char(c.captured)));
    }
    if c.gives_check {
        tags.push("叫将".to_string());
    }
    if tags.is_empty() {
        c.notation.clone()
    } else {
        format!("{}（{}）", c.notation, tags.join("、"))
    }
}

/// 局面讲解的数据上下文（喂给 AI 或降级模板）。
pub struct PositionCtx {
    pub fen: String,
    pub red_to_move: bool,
    pub last_notation: Option<String>,
    pub last_intent_hint: String,
    pub best: Candidate,
    pub best_reason: String,
    pub alternatives: Vec<(Candidate, i32)>, // (走法, 比最佳亏多少分)
    pub score: i32,
}

/// 单枚棋子讲解的数据上下文。
pub struct PieceCtx {
    pub fen: String,
    pub piece: i8,
    pub from_sq: usize,
    pub best: Candidate,
    pub best_reason: String,
    pub alternatives: Vec<(Candidate, i32)>,
}

pub fn build_position_prompt(ctx: &PositionCtx) -> String {
    let alts = ctx
        .alternatives
        .iter()
        .take(4)
        .map(|(c, loss)| format!("{} 亏约 {} 分", describe_candidate(c), loss))
        .collect::<Vec<_>>()
        .join("；");
    format!(
        "当前局面 FEN：{}\n轮到{}走棋（学员执红）。\n{}引擎分析结论：最佳走法是 {}，局面评分约 {} 分，棋理标签：{}。\n其他候选走法及亏损：{}。\n请给学员讲清楚：1) 对方上一步的意图；2) 为什么 {} 是当前最佳；3) 哪些走法是坑、坑在哪。",
        ctx.fen,
        if ctx.red_to_move { "红方" } else { "黑方" },
        match &ctx.last_notation {
            Some(n) => format!("对方刚走了 {}。{}\n", n, ctx.last_intent_hint),
            None => "这是对局开始。\n".to_string(),
        },
        describe_candidate(&ctx.best),
        ctx.score,
        ctx.best_reason,
        if alts.is_empty() { "无明显大亏走法".into() } else { alts },
        ctx.best.notation,
    )
}

pub fn build_piece_prompt(ctx: &PieceCtx) -> String {
    let alts = ctx
        .alternatives
        .iter()
        .take(3)
        .map(|(c, loss)| format!("{} 亏约 {} 分", describe_candidate(c), loss))
        .collect::<Vec<_>>()
        .join("；");
    format!(
        "当前局面 FEN：{}\n学员选中了自己的{}（位置：第{}行第{}列），犹豫要不要走它。\n引擎分析结论：这枚{}的最佳走法是 {}，棋理标签：{}。其他走法及亏损：{}。\n请给学员讲清楚：这枚{}现在该怎么走、为什么这么走、其他走法分别有什么问题。",
        ctx.fen,
        piece_char(ctx.piece),
        row(ctx.from_sq) + 1,
        col(ctx.from_sq) + 1,
        piece_char(ctx.piece),
        describe_candidate(&ctx.best),
        ctx.best_reason,
        if alts.is_empty() { "其余走法差别不大".into() } else { alts },
        piece_char(ctx.piece),
    )
}

/// 根据走法特征推断一句话"棋理"理由（模板降级用）。
pub fn reason_of(b_before: &Board, c: &Candidate) -> String {
    let mut reasons = Vec::new();
    if c.captured != 0 {
        reasons.push(format!("吃掉对方的{}", piece_char(c.captured)));
    }
    if c.gives_check {
        reasons.push("叫将，抢占先手".to_string());
    }
    let (fr, tr) = (row(c.mv.from), row(c.mv.to));
    let (fc, tc) = (col(c.mv.from), col(c.mv.to));
    let p = b_before.cells[c.mv.from];
    if reasons.is_empty() {
        let red = p > 0;
        // 出子：离开底线/次底线的大子
        let back_rank = if red { fr >= 8 } else { fr <= 1 };
        if back_rank && matches!(p.abs(), R | H | C) {
            reasons.push("出动大子，加快出子速度".to_string());
        }
        // 过河
        if matches!(p.abs(), P | H | R | C) && (if red { tr <= 4 && fr > 4 } else { tr >= 5 && fr < 5 }) {
            reasons.push("子力过河，投入进攻".to_string());
        }
        if tc == 4 && fc != 4 {
            reasons.push("抢占中路，控制要道".to_string());
        }
        if reasons.is_empty() {
            reasons.push("调整子力位置，巩固阵型".to_string());
        }
    }
    reasons.join("，")
}

pub fn fallback_position(ctx: &PositionCtx) -> String {
    let mut out = String::new();
    match &ctx.last_notation {
        Some(n) => out.push_str(&format!("对方刚走了 {}。{}\n", n, ctx.last_intent_hint)),
        None => out.push_str("对局刚开始，轮到你走棋。\n"),
    }
    let score_text = if ctx.score >= 200 {
        "你明显占优"
    } else if ctx.score >= 60 {
        "你稍占优"
    } else if ctx.score > -60 {
        "双方大体均势"
    } else if ctx.score > -200 {
        "你稍处下风"
    } else {
        "你明显被动"
    };
    out.push_str(&format!("当前局面{}（引擎评分 {} 分）。\n", score_text, ctx.score));
    out.push_str(&format!(
        "建议走 {}：{}（评分约 {} 分）。\n",
        ctx.best.notation, ctx.best_reason, ctx.score
    ));
    let traps: Vec<String> = ctx
        .alternatives
        .iter()
        .filter(|(_, loss)| *loss >= 80)
        .take(2)
        .map(|(c, loss)| format!("「{}」会亏约 {} 分", c.notation, loss))
        .collect();
    if !traps.is_empty() {
        out.push_str(&format!("注意避开：{}。\n", traps.join("，")));
    }
    out.push_str("\n（这是引擎基础提示。在设置里填入 AI 服务 Key，即可获得完整的大白话棋理讲解。）");
    out
}

pub fn fallback_piece(ctx: &PieceCtx) -> String {
    let mut out = format!(
        "这枚{}当前的最佳走法是 {}：{}（评分约 {} 分）。\n",
        piece_char(ctx.piece),
        ctx.best.notation,
        ctx.best_reason,
        ctx.best.score
    );
    let traps: Vec<String> = ctx
        .alternatives
        .iter()
        .filter(|(_, loss)| *loss >= 80)
        .take(3)
        .map(|(c, loss)| format!("「{}」会亏约 {} 分", c.notation, loss))
        .collect();
    if !traps.is_empty() {
        out.push_str(&format!("要注意的差走法：{}。\n", traps.join("；")));
    } else if ctx.alternatives.len() > 1 {
        out.push_str("这枚棋的其他走法差别不大，可以放心走最佳走法。\n");
    }
    out.push_str("\n（这是引擎基础提示。配置 AI 服务后，这里会有完整棋理讲解。）");
    out
}

/// 构造局面讲解上下文：分析 + 上一步意图推断。
pub fn make_position_ctx(
    b: &Board,
    analysis: &Analysis,
    last: Option<(Move, String, i8)>, // (走法, 记谱, 被吃的子)
    depth_board: &Board,
) -> Option<PositionCtx> {
    let best = analysis.best.clone()?;
    let best_score = best.score;
    let alternatives: Vec<(Candidate, i32)> = analysis
        .candidates
        .iter()
        .skip(1)
        .map(|c| (c.clone(), best_score - c.score))
        .collect();
    let last_intent_hint = match &last {
        Some((mv, _, captured)) => {
            let mut hints = Vec::new();
            if *captured != 0 {
                hints.push(format!("这步吃掉了你的{}", piece_char(*captured)));
            }
            let (tr, tc) = (row(mv.to), col(mv.to));
            if tc == 4 {
                hints.push("占据中路".to_string());
            }
            if depth_board.in_check(depth_board.red_turn) {
                hints.push("正在将军".to_string());
            }
            let _ = tr;
            if hints.is_empty() {
                "这步在调动子力，意图还需观察。".to_string()
            } else {
                format!("这步{}。", hints.join("，"))
            }
        }
        None => String::new(),
    };
    Some(PositionCtx {
        fen: b.to_fen(),
        red_to_move: b.red_turn,
        last_notation: last.map(|(_, n, _)| n),
        last_intent_hint,
        best_reason: reason_of(b, &best),
        best,
        alternatives,
        score: best_score,
    })
}

pub fn make_piece_ctx(b: &Board, from: usize, analysis: &Analysis) -> Option<PieceCtx> {
    let best = analysis.best.clone()?;
    let best_score = best.score;
    let alternatives: Vec<(Candidate, i32)> = analysis
        .candidates
        .iter()
        .skip(1)
        .map(|c| (c.clone(), best_score - c.score))
        .collect();
    Some(PieceCtx {
        fen: b.to_fen(),
        piece: b.cells[from],
        from_sq: from,
        best_reason: reason_of(b, &best),
        best,
        alternatives,
    })
}
