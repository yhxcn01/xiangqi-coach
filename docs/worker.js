// 象棋思考教练 — WASM 引擎宿主。
// 所有引擎搜索都是同步阻塞调用，统一放进这个 Worker，避免卡住页面 UI。

let ready = null, exports = null, memory = null;
const enc = new TextEncoder(), dec = new TextDecoder();

function init() {
  if (!ready) {
    ready = (async () => {
      const buf = await (await fetch('xqwasm.wasm?v=5')).arrayBuffer();
      const { instance } = await WebAssembly.instantiate(buf);
      exports = instance.exports;
      memory = exports.memory;
    })();
  }
  return ready;
}

// 约定：输入为走法列表 JSON（写入 WASM 线性内存），输出为 '\0' 结尾的 UTF-8 JSON。
function run(name, inputJson, ...extra) {
  const bytes = enc.encode(inputJson == null ? '' : inputJson);
  const ptr = exports.xq_alloc(bytes.length);
  if (bytes.length) new Uint8Array(memory.buffer, ptr, bytes.length).set(bytes);
  const outPtr = exports[name](ptr, bytes.length, ...extra);
  exports.xq_dealloc(ptr, bytes.length);
  const view = new Uint8Array(memory.buffer, outPtr);
  let end = view.indexOf(0);
  if (end < 0) end = view.length;
  const s = dec.decode(view.subarray(0, end));
  exports.xq_free(outPtr);
  return JSON.parse(s);
}

self.onmessage = async (e) => {
  const { id, cmd, moves, from, to, sq, depth, turn } = e.data;
  try {
    await init();
    const mj = JSON.stringify(moves || []);
    let data;
    switch (cmd) {
      case 'state':            data = run('xq_state', mj); break;
      case 'legal':            data = run('xq_legal', mj, sq); break;
      case 'move':             data = run('xq_move', mj, from, to); break;
      case 'engine':           data = run('xq_engine_reply', mj, depth || 4); break;
      case 'explain_position': data = run('xq_explain_position', mj, depth || 4); break;
      case 'explain_piece':    data = run('xq_explain_piece', mj, sq, depth || 4); break;
      case 'review':           data = run('xq_review_turn', mj, turn, depth || 3); break;
      case 'rate':             data = run('xq_rate_last', mj, depth || 3); break;
      default: throw new Error('未知命令 ' + cmd);
    }
    if (data && data.error) throw new Error(data.error);
    postMessage({ id, ok: true, data });
  } catch (err) {
    postMessage({ id, ok: false, error: String((err && err.message) || err) });
  }
};
