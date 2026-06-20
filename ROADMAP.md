# 🗺️ ROADMAP — agents-rust

> Mục tiêu: Nâng cấp từ single-agent loop lên multi-agent orchestration platform cho Deep Research.
> Tham khảo kiến trúc: [DeerFlow](https://github.com/bytedance/deer-flow) (ByteDance, 71.8k ⭐) + [Anthropic Building Effective Agents](https://anthropic.com/engineering/building-effective-agents)

---

## 📋 Điểm yếu & thiếu sót hiện tại

| Khoản | Hiện tại | Cần bổ sung |
|-------|----------|-------------|
| **Multi-Agent** | Single agent loop | Orchestrator + workers pattern (như DeerFlow) |
| **Streaming realtime** | Có chat_stream nhưng chưa gắn vào agent loop | WebSocket stream ra UI |
| **Dashboard** | Không có | Web UI realtime (React/Tauri) |
| **Memory persistence** | Không có | Vector store (Qdrant/Surrealdb) + long-term memory |
| **Sandbox isolation** | Chạy trực tiếp trên host | Docker sandbox cho execution |
| **Context management** | Agent loop gom hết messages | Summarization + context compression |
| **Skills system** | Cứng — tools hardcode trong mod.rs | Skills động load như DeerFlow |
| **Web search** | Chỉ DuckDuckGo (có thể bị block) | Search API chuyên nghiệp (Brave/Exa/Google) |

---

## 🔍 So sánh với DeerFlow

| Tính năng | DeerFlow | agents-rust |
|-----------|----------|-------------|
| Sub-agents | Spawn động, parallel | ❌ |
| Skills (.md) | Load skill động theo task | ❌ tools cứng |
| Sandbox Docker | Isolated container | ❌ |
| Long-term memory | Vector store | ❌ |
| Context engineering | Summarization + compression | ❌ |
| Web UI | Frontend React | ❌ |
| IM channels | Slack, Discord, Telegram | ❌ |
| Execution modes | Flash / Standard / Pro / Ultra | ❌ chỉ 1 mode |

---

## 🧭 Lộ trình — 3 Phase

### Phase 1: 🏗️ Nền tảng (ưu tiên nhất)

**1. Multi-Agent Orchestrator**
- Spawn async worker tasks (`tokio::spawn`)
- Mỗi worker có independent context + tool set
- Lead agent tổng hợp kết quả từ các sub-agent
- Chạy parallel khi được

**2. WebSocket Streaming Dashboard**
- Dùng `tokio-tungstenite` (đã có trong Cargo.toml)
- Agent push log realtime qua WebSocket
- Web UI đơn giản (HTML/JS thuần hoặc Tauri)

### Phase 2: 🔧 Cốt lõi

**3. Long-term Memory**
- Vector store: Qdrant (Rust native) hoặc SQLite + embeddings
- Embedding: `fastembed` (chạy local, không cần API)
- Lưu research history, user preferences, kết quả trước đó

**4. Web Search chuyên nghiệp**
- Brave Search API hoặc Exa (đã có websearch MCP)
- Google/Bing Search API
- Fallback tự động nếu 1 service bị lỗi

**5. Skills System động**
- Load skill từ file `.md` (giống DeerFlow)
- Skill registry động — đăng ký tool theo task
- Skill có: mô tả, tools, instructions riêng

### Phase 3: 🚀 Production

**6. Sandbox Docker**
- Mỗi task chạy trong container riêng (dùng `bollard` — Rust SDK cho Docker)
- Resource limits (CPU, RAM, timeout)
- Dọn dẹp container sau khi hoàn thành

**7. Context Engineering**
- Summarize completed sub-tasks
- Compress conversation history
- Chỉ giữ lại thông tin quan trọng cho agent loop

**8. Multi-provider LLM**
- OpenAI, Claude, Gemini, OpenRouter
- Auto fallback khi 1 provider lỗi
- Load balancing theo cost/latency

---

## 🎯 Công nghệ & thư viện tham khảo

| Mục | Công nghệ đề xuất | Lý do |
|-----|------------------|-------|
| Vector store | `qdrant` | Rust native, nhẹ, mạnh |
| Embedding | `fastembed` | Chạy local, không cần API key |
| Web search API | Brave Search API hoặc Exa | Đã có websearch MCP |
| Frontend | Tauri + React | Rust backend + Web UI native |
| Streaming | WebSocket (`tokio-tungstenite`) | Đã có trong Cargo.toml |
| Sandbox | `bollard` (Docker API) | Rust SDK cho Docker |
| Multi-agent pattern | DeerFlow design | github.com/bytedance/deer-flow |
| Agent design principles | Anthropic building effective agents | anthropic.com/engineering/building-effective-agents |

---

## 💬 Kết luận

`agents-rust` đã có core rất tốt: agent loop, Obscura CDP, MCP server/client, WASM plugin — clean và đúng architecture.

Cái còn thiếu chủ yếu:
1. **Multi-agent** — quan trọng nhất cho Deep Research
2. **Dashboard / streaming** — để thấy agent đang làm gì
3. **Memory persistence** — để không quên sau mỗi lần
4. **Skills động** — thay vì hardcode tools
