//! 象棋思考教练 WASM 导出层（C ABI + JSON）。
//!
//! 设计约定（配合 web/worker.js 使用）：
//! - 无状态：所有导出以「完整走法列表」为输入，JSON 数组 `[[from,to],...]`，
//!   由 JS 先写入线性内存（xq_alloc 申请、xq_dealloc 释放），再把指针和长度传进来。
//! - 输出为 '\0' 结尾的 UTF-8 JSON 字符串指针（JS 读取后必须调用 xq_free 释放）。
//! - 失败统一返回 `{"error":"..."}`，由 JS 侧判别。
//! - 搜索都是同步阻塞调用，必须在 Web Worker 里运行，避免卡 UI。

use std::os::raw::{c_char, c_int};

use serde::Serialize;
use xqcore::coach;
use xqcore::engine;
use xqcore::xiangqi::{Board, Move};

// ---------- 内存管理 ----------

/// 供 JS 申请输入缓冲区。
#[no_mangle]
pub extern "C" fn xq_alloc(len: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(len.max(1));
    let ptr = v.as_mut_ptr();
    std::mem::forget(v);
    ptr
}

/// 释放 JS 申请过的输入缓冲区。
#[no_mangle]
pub extern "C" fn xq_dealloc(ptr: *mut u8, len: usize) {
    if ptr.is_null() {
        return;
    }
    unsafe {
        drop(Vec::from_raw_parts(ptr, 0, len.max(1)));
    }
}

/// 释放某个输出字符串指针。
#[no_mangle]
pub extern "C" fn xq_free(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            drop(std::ffi::CString::from_raw(ptr));
        }
    }
}

// ---------- 内部工具 ----------

fn read_input(ptr: *const u8, len: usize) -> Result<Vec<Move>, String> {
    if len == 0 || ptr.is_null() {
        return Ok(Vec::new());
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    serde_json::from_slice(bytes).map_err(|_| "走法列表 JSON 无效".to_string())
}

fn emit<T: Serialize>(v: &T) -> *mut c_char {
    let s = serde_json::to_string(v).unwrap_or_else(|_| r#"{"error":"序列化失败"}"#.into());
    // JSON 文本不会含 '\0'，稳妥替换避免 CString 失败
    let s = s.replace('\0', " ");
    match std::ffi::CString::new(s) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ffi::CString::new(r#"{"error":"输出编码失败"}"#)
            .unwrap()
            .into_raw(),
    }
}

fn emit_err(msg: &str) -> *mut c_char {
    emit(&serde_json::json!({ "error": msg }))
}

fn replay(moves: &[Move]) -> Result<(Board, Vec<(Move, String, i8)>), String> {
    let mut b = Board::start();
    let mut hist = Vec::with_capacity(moves.len());
    for &mv in moves {
        if !b.legal_moves().contains(&mv) {
            return Err(format!("非法走法：{}→{}", mv.from, mv.to));
        }
        let notation = b.move_notation(mv);
        let captured = b.make(mv);
        hist.push((mv, notation, captured));
    }
    Ok((b, hist))
}

fn game_over_text(b: &Board) -> Option<String> {
    if b.legal_moves().is_empty() {
        let loser = if b.red_turn { "红方" } else { "黑方" };
        let winner = if b.red_turn { "黑方" } else { "红方" };
        let why = if b.in_check(b.red_turn) { "绝杀" } else { "困毙" };
        Some(format!("{}！{}获胜（{}无棋可走）", why, winner, loser))
    } else {
        None
    }
}

#[derive(Serialize)]
struct StateOut {
    cells: Vec<i8>,
    red_turn: bool,
    history: Vec<String>,
    game_over: Option<String>,
    in_check: bool,
    fen: String,
    /// 仅 xq_engine_reply 携带：引擎刚走的着法
    engine_move: Option<Move>,
}

fn state_out(b: &Board, hist: &[(Move, String, i8)]) -> StateOut {
    StateOut {
        cells: b.cells.to_vec(),
        red_turn: b.red_turn,
        history: hist.iter().map(|(_, n, _)| n.clone()).collect(),
        game_over: game_over_text(b),
        in_check: b.in_check(b.red_turn),
        fen: b.to_fen(),
        engine_move: None,
    }
}

// ---------- 导出：状态与走棋 ----------

/// 按走法列表重放出当前状态。
#[no_mangle]
pub extern "C" fn xq_state(in_ptr: *const u8, in_len: usize) -> *mut c_char {
    let moves = match read_input(in_ptr, in_len) {
        Ok(m) => m,
        Err(e) => return emit_err(&e),
    };
    match replay(&moves) {
        Ok((b, hist)) => emit(&state_out(&b, &hist)),
        Err(e) => emit_err(&e),
    }
}

/// 指定格子的合法目标（当前走棋方）。
#[no_mangle]
pub extern "C" fn xq_legal(in_ptr: *const u8, in_len: usize, square: c_int) -> *mut c_char {
    let moves = match read_input(in_ptr, in_len) {
        Ok(m) => m,
        Err(e) => return emit_err(&e),
    };
    let board = match replay(&moves) {
        Ok((b, _)) => b,
        Err(e) => return emit_err(&e),
    };
    let targets = if (0..90).contains(&square) {
        board.legal_targets(square as usize)
    } else {
        vec![]
    };
    emit(&serde_json::json!({ "targets": targets }))
}

/// 玩家走一步（服务端校验合法性），返回新状态。
#[no_mangle]
pub extern "C" fn xq_move(in_ptr: *const u8, in_len: usize, from: c_int, to: c_int) -> *mut c_char {
    let mut moves = match read_input(in_ptr, in_len) {
        Ok(m) => m,
        Err(e) => return emit_err(&e),
    };
    if !(0..90).contains(&from) || !(0..90).contains(&to) {
        return emit_err("坐标越界");
    }
    moves.push(Move { from: from as usize, to: to as usize });
    match replay(&moves) {
        Ok((b, hist)) => emit(&state_out(&b, &hist)),
        Err(e) => emit_err(&e),
    }
}

/// 引擎（黑方）应着一步，返回带 engine_move 的新状态。
#[no_mangle]
pub extern "C" fn xq_engine_reply(in_ptr: *const u8, in_len: usize, depth: c_int) -> *mut c_char {
    let moves = match read_input(in_ptr, in_len) {
        Ok(m) => m,
        Err(e) => return emit_err(&e),
    };
    let (mut board, mut hist) = match replay(&moves) {
        Ok(x) => x,
        Err(e) => return emit_err(&e),
    };
    if game_over_text(&board).is_some() {
        return emit_err("对局已结束");
    }
    let analysis = engine::analyze(&board, depth);
    let best = match analysis.best {
        Some(b) => b,
        None => return emit_err("无棋可走"),
    };
    let notation = board.move_notation(best.mv);
    let captured = board.make(best.mv);
    hist.push((best.mv, notation, captured));
    let mut out = state_out(&board, &hist);
    out.engine_move = Some(best.mv);
    emit(&out)
}

// ---------- 导出：讲解 ----------

/// 局面讲解上下文：返回 {prompt, fallback}。prompt 给 LLM，fallback 为无 Key/失败时的模板。
#[no_mangle]
pub extern "C" fn xq_explain_position(in_ptr: *const u8, in_len: usize, depth: c_int) -> *mut c_char {
    let moves = match read_input(in_ptr, in_len) {
        Ok(m) => m,
        Err(e) => return emit_err(&e),
    };
    let (board, hist) = match replay(&moves) {
        Ok(x) => x,
        Err(e) => return emit_err(&e),
    };
    let analysis = engine::analyze(&board, depth);
    let recent: Vec<String> = hist.iter().map(|(_, n, _)| n.clone()).collect();
    let ctx = match coach::make_position_ctx(&board, &analysis, hist.last().cloned(), &board, &recent) {
        Some(c) => c,
        None => return emit_err("当前无棋可走"),
    };
    emit(&serde_json::json!({
        "system": coach::SYSTEM_PROMPT,
        "prompt": coach::build_position_prompt(&ctx),
        "fallback": coach::fallback_position(&ctx),
    }))
}

/// 单枚棋子讲解上下文：返回 {system, prompt, fallback, mode}。
/// 己方棋子讲"该怎么走"；对方棋子翻转回合分析，讲"它的威胁与应对"。
#[no_mangle]
pub extern "C" fn xq_explain_piece(
    in_ptr: *const u8,
    in_len: usize,
    square: c_int,
    depth: c_int,
) -> *mut c_char {
    let moves = match read_input(in_ptr, in_len) {
        Ok(m) => m,
        Err(e) => return emit_err(&e),
    };
    let (board, _) = match replay(&moves) {
        Ok(x) => x,
        Err(e) => return emit_err(&e),
    };
    if !(0..90).contains(&square) || board.cells[square as usize] == 0 {
        return emit_err("这里没有棋子");
    }
    let square = square as usize;
    let own = (board.cells[square] > 0) == board.red_turn;
    let analysis_board = if own {
        board.clone()
    } else {
        let mut flipped = board.clone();
        flipped.red_turn = !flipped.red_turn;
        flipped
    };
    let analysis = engine::analyze_piece(&analysis_board, square, depth);
    if analysis.candidates.is_empty() {
        return emit_err("这枚棋子当前没有合法走法");
    }
    if own {
        let ctx = match coach::make_piece_ctx(&board, square, &analysis) {
            Some(c) => c,
            None => return emit_err("分析失败"),
        };
        emit(&serde_json::json!({
            "mode": "own",
            "system": coach::SYSTEM_PROMPT,
            "prompt": coach::build_piece_prompt(&ctx),
            "fallback": coach::fallback_piece(&ctx),
        }))
    } else {
        let ctx = match coach::make_opponent_piece_ctx(&board, square, &analysis) {
            Some(c) => c,
            None => return emit_err("分析失败"),
        };
        emit(&serde_json::json!({
            "mode": "opponent",
            "system": coach::SYSTEM_PROMPT,
            "prompt": coach::build_opponent_piece_prompt(&ctx),
            "fallback": coach::fallback_opponent_piece(&ctx),
        }))
    }
}

// ---------- 导出：复盘 ----------

/// 复盘红方第 turn 手（0 起，对应走法列表下标 2*turn）：
/// 分析该步之前的局面，对比实际着法与引擎最佳，返回亏分与建议。
#[no_mangle]
pub extern "C" fn xq_review_turn(in_ptr: *const u8, in_len: usize, turn: c_int, depth: c_int) -> *mut c_char {
    let moves = match read_input(in_ptr, in_len) {
        Ok(m) => m,
        Err(e) => return emit_err(&e),
    };
    let idx = turn as usize * 2;
    if idx >= moves.len() {
        return emit_err("超出着法范围");
    }
    let (board, _) = match replay(&moves[..idx]) {
        Ok(x) => x,
        Err(e) => return emit_err(&e),
    };
    let analysis = engine::analyze(&board, depth);
    let best = match analysis.best {
        Some(b) => b,
        None => return emit_err("该局面无棋可走，无需复盘"),
    };
    let actual_mv = moves[idx];
    let actual = analysis
        .candidates
        .iter()
        .find(|c| c.mv == actual_mv)
        .ok_or(())
        .cloned();
    let actual = match actual {
        Ok(a) => a,
        Err(()) => return emit_err("该着法不在候选中"),
    };
    let loss = (best.score - actual.score).max(0);
    emit(&serde_json::json!({
        "turn": turn,
        "notation": actual.notation,
        "loss": loss,
        "best": coach::describe_candidate(&best),
        "best_reason": coach::reason_of(&board, &best),
        "score": actual.score,
        "gives_check": actual.gives_check,
    }))
}
