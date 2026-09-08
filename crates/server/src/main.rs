//! 象棋思考教练 —— Web 服务层。
//! 引擎是大脑，AI 是嘴：所有走法结论由内置引擎算出，AI 只负责讲解。

mod coach;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tower_http::services::ServeDir;
use xqcore::engine;
use xqcore::xiangqi::{Board, Move};

struct GameState {
    board: Board,
    /// 历史：走法、记谱、被吃的子
    history: Vec<(Move, String, i8)>,
    game_over: Option<String>,
}

struct Shared {
    game: Mutex<GameState>,
    settings: Mutex<coach::Settings>,
}

#[derive(Serialize)]
struct StateResp {
    cells: Vec<i8>,
    red_turn: bool,
    history: Vec<String>,
    game_over: Option<String>,
    has_ai: bool,
    in_check: bool,
}

fn state_resp(sh: &Shared) -> StateResp {
    let g = sh.game.lock().unwrap();
    let s = sh.settings.lock().unwrap();
    StateResp {
        cells: g.board.cells.to_vec(),
        red_turn: g.board.red_turn,
        history: g.history.iter().map(|(_, n, _)| n.clone()).collect(),
        game_over: g.game_over.clone(),
        has_ai: s.has_ai(),
        in_check: g.board.in_check(g.board.red_turn),
    }
}

fn check_game_over(g: &mut GameState) {
    if g.board.legal_moves().is_empty() {
        let loser = if g.board.red_turn { "红方" } else { "黑方" };
        let winner = if g.board.red_turn { "黑方" } else { "红方" };
        let why = if g.board.in_check(g.board.red_turn) {
            "绝杀"
        } else {
            "困毙"
        };
        g.game_over = Some(format!("{}！{}获胜（{}无棋可走）", why, winner, loser));
    }
}

async fn get_state(State(sh): State<Arc<Shared>>) -> Json<StateResp> {
    Json(state_resp(&sh))
}

#[derive(Deserialize)]
struct MoveReq {
    from: usize,
    to: usize,
}

async fn do_move(State(sh): State<Arc<Shared>>, Json(req): Json<MoveReq>) -> Result<Json<StateResp>, (StatusCode, String)> {
    let mut g = sh.game.lock().unwrap();
    if g.game_over.is_some() {
        return Err((StatusCode::BAD_REQUEST, "对局已结束，请开始新对局".into()));
    }
    let mv = Move { from: req.from, to: req.to };
    if !g.board.legal_moves().contains(&mv) {
        return Err((StatusCode::BAD_REQUEST, "非法走法".into()));
    }
    let notation = g.board.move_notation(mv);
    let captured = g.board.make(mv);
    g.history.push((mv, notation, captured));
    check_game_over(&mut g);
    drop(g);
    Ok(Json(state_resp(&sh)))
}

async fn engine_move(State(sh): State<Arc<Shared>>) -> Result<Json<StateResp>, (StatusCode, String)> {
    let (board, over) = {
        let g = sh.game.lock().unwrap();
        (g.board.clone(), g.game_over.clone())
    };
    if over.is_some() {
        return Err((StatusCode::BAD_REQUEST, "对局已结束".into()));
    }
    let depth = engine::default_depth();
    let analysis = tokio::task::spawn_blocking(move || engine::analyze(&board, depth))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let best = analysis
        .best
        .ok_or((StatusCode::BAD_REQUEST, "无棋可走".to_string()))?;
    let mut g = sh.game.lock().unwrap();
    let notation = g.board.move_notation(best.mv);
    let captured = g.board.make(best.mv);
    g.history.push((best.mv, notation, captured));
    check_game_over(&mut g);
    drop(g);
    Ok(Json(state_resp(&sh)))
}

async fn undo(State(sh): State<Arc<Shared>>) -> Json<StateResp> {
    let mut g = sh.game.lock().unwrap();
    // 撤销两步（玩家+引擎），若只有一步则撤销一步
    let n = if g.history.len() >= 2 { 2 } else { g.history.len() };
    for _ in 0..n {
        if let Some((mv, _, _)) = g.history.pop() {
            // 逆转 make：交换回来并恢复被吃子 —— 通过重放重建更稳妥
            let _ = mv;
            let mut b = Board::start();
            let mut hist = Vec::new();
            for (m, _, _) in g.history.clone() {
                let notation = b.move_notation(m);
                let cap = b.make(m);
                hist.push((m, notation, cap));
            }
            g.board = b;
            g.history = hist;
            g.game_over = None;
        }
    }
    drop(g);
    Json(state_resp(&sh))
}

async fn new_game(State(sh): State<Arc<Shared>>) -> Json<StateResp> {
    let mut g = sh.game.lock().unwrap();
    g.board = Board::start();
    g.history.clear();
    g.game_over = None;
    drop(g);
    Json(state_resp(&sh))
}

async fn legal_moves(
    State(sh): State<Arc<Shared>>,
    Path(sq): Path<usize>,
) -> Json<serde_json::Value> {
    let g = sh.game.lock().unwrap();
    let targets = if sq < 90 { g.board.legal_targets(sq) } else { vec![] };
    Json(serde_json::json!({ "targets": targets }))
}

#[derive(Serialize)]
struct ExplainResp {
    text: String,
    source: String,
}

async fn explain_position(State(sh): State<Arc<Shared>>) -> Result<Json<ExplainResp>, (StatusCode, String)> {
    let (board, last) = {
        let g = sh.game.lock().unwrap();
        (g.board.clone(), g.history.last().cloned())
    };
    let depth = engine::default_depth();
    let b2 = board.clone();
    let analysis = tokio::task::spawn_blocking(move || engine::analyze(&b2, depth))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let ctx = xqcore::coach::make_position_ctx(&board, &analysis, last, &board)
        .ok_or((StatusCode::BAD_REQUEST, "当前无棋可走".to_string()))?;
    let settings = sh.settings.lock().unwrap().clone();
    let (text, source) = coach::explain_position(&settings, &ctx).await;
    Ok(Json(ExplainResp {
        text,
        source: source.to_string(),
    }))
}

#[derive(Deserialize)]
struct PieceReq {
    square: usize,
}

async fn explain_piece(
    State(sh): State<Arc<Shared>>,
    Json(req): Json<PieceReq>,
) -> Result<Json<ExplainResp>, (StatusCode, String)> {
    let board = { sh.game.lock().unwrap().board.clone() };
    if req.square >= 90 || board.cells[req.square] == 0 {
        return Err((StatusCode::BAD_REQUEST, "这里没有棋子".into()));
    }
    if (board.cells[req.square] > 0) != board.red_turn {
        return Err((StatusCode::BAD_REQUEST, "现在不能走对方的棋子".into()));
    }
    let depth = engine::default_depth();
    let b2 = board.clone();
    let from = req.square;
    let analysis = tokio::task::spawn_blocking(move || engine::analyze_piece(&b2, from, depth))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if analysis.candidates.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "这枚棋子当前没有合法走法".into()));
    }
    let ctx = xqcore::coach::make_piece_ctx(&board, req.square, &analysis)
        .ok_or((StatusCode::BAD_REQUEST, "分析失败".to_string()))?;
    let settings = sh.settings.lock().unwrap().clone();
    let (text, source) = coach::explain_piece(&settings, &ctx).await;
    Ok(Json(ExplainResp {
        text,
        source: source.to_string(),
    }))
}

async fn get_settings(State(sh): State<Arc<Shared>>) -> Json<serde_json::Value> {
    let s = sh.settings.lock().unwrap();
    Json(serde_json::json!({
        "base_url": s.base_url,
        "model": s.model,
        "has_key": s.has_ai(),
    }))
}

#[derive(Deserialize)]
struct SettingsReq {
    base_url: Option<String>,
    api_key: Option<String>,
    model: Option<String>,
}

async fn save_settings(
    State(sh): State<Arc<Shared>>,
    Json(req): Json<SettingsReq>,
) -> Json<serde_json::Value> {
    let mut s = sh.settings.lock().unwrap();
    if let Some(v) = req.base_url {
        if !v.trim().is_empty() {
            s.base_url = v.trim().trim_end_matches('/').to_string();
        }
    }
    if let Some(v) = req.api_key {
        s.api_key = v.trim().to_string();
    }
    if let Some(v) = req.model {
        if !v.trim().is_empty() {
            s.model = v.trim().to_string();
        }
    }
    s.save();
    let has = s.has_ai();
    drop(s);
    Json(serde_json::json!({ "ok": true, "has_key": has }))
}

#[tokio::main]
async fn main() {
    let mut port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(7100);
    let args: Vec<String> = std::env::args().collect();
    for i in 0..args.len() {
        if args[i] == "--port" && i + 1 < args.len() {
            if let Ok(p) = args[i + 1].parse() {
                port = p;
            }
        }
    }

    let shared = Arc::new(Shared {
        game: Mutex::new(GameState {
            board: Board::start(),
            history: Vec::new(),
            game_over: None,
        }),
        settings: Mutex::new(coach::Settings::load()),
    });

    let app = Router::new()
        .route("/api/state", get(get_state))
        .route("/api/move", post(do_move))
        .route("/api/engine_move", post(engine_move))
        .route("/api/undo", post(undo))
        .route("/api/new", post(new_game))
        .route("/api/legal/:sq", get(legal_moves))
        .route("/api/explain/position", post(explain_position))
        .route("/api/explain/piece", post(explain_piece))
        .route("/api/settings", get(get_settings).post(save_settings))
        .fallback_service(ServeDir::new("static"))
        .with_state(shared);

    let addr = format!("0.0.0.0:{}", port);
    println!("象棋思考教练已启动：http://localhost:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
