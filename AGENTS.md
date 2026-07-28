# AGENTS.md

## Commands

ツールチェインのコマンドは `mise x -- <cmd>` を使う。

- セットアップ: `mise run setup` (OpenCV 4.10 静的ビルド + submodule 初期化 + `bun install`)
- デスクトップアプリを起動: `mise run dev` (サイドカー含む)
  - フロントエンドだけ起動したい場合: `mise run dev:web` (port 1420)
- フロントエンドのみビルド: `mise run build`
- サイドカー (tooth-backend) のみビルド: `mise run build:core-tools`
- サイドカー + デスクトップアプリをビルド: `mise run build:all`
- 型検査: `mise run type`
- リント・フォーマット修正: `mise run lint`
- Rust 側の検証 (`src-tauri/`)
  - `cargo fmt --all`
  - `cargo clippy`
  - または `cargo lint` / `cargo format`
- 全体チェック: `mise run check` (Rust + TypeScript + core-tools)

### core-tools (tooth-backend) ビルド

- ホストマシンで直接ビルド
- 前提: OpenCV 4.10 静的ビルドが `~/.local/opencv/4.10.0-static` にあること (`mise run setup` または `mise run build:opencv` で構築)
- システム依存: `pkg-config`, `cmake`, `ninja`, `libgtk-3-dev`, `liblapack-dev`, `libblas-dev`, `libcblas-dev`, `liblapacke-dev`, `git`
- 成果物: `src-tauri/binaries/tooth-backend-<host-tuple>`

## Project Structure

```
/
├─ src/                 # フロントエンドコード
│    ├─ main.tsx       # フロントエンドエントリ
│    ├─ routes/        # TanStack Router のルート
│    └─ lib/           # フロントエンド用ユーティリティ
└─ src-tauri/           # Rust コードと Tauri 設定 (詳細は src-tauri/AGENTS.md 参照)
```

Rust 側の構造・規約は [`src-tauri/AGENTS.md`](./src-tauri/AGENTS.md) を参照。

## Code Style

### TypeScript

- any や型アサーションは使用しない
- 自動フォーマットが可能な場合は自動フォーマットを行うこと

### Rust

- `src-tauri/AGENTS.md` を参照

## Boundaries

### Always do

- 変更後にリント系のチェックを通す

### Ask first

- 依存パッケージの追加、メジャーバージョン更新
- tauri.config.json 、 tsconfig.json 、 biome.json などの設定ファイルの更新

### Never do

- node_modules 等の生成物を編集
- リントエラー等の ignore
