# AGENTS.md

## Commands

ツールチェインのコマンドは `mise x -- <cmd>` を使う。

- デスクトップアプリを起動: `bun tauri dev`
  - フロントエンドだけ起動したい場合: `bun dev` (port 1420)
- フロントエンドのみビルド: `bun build`
- デスクトップアプリをビルド: `bun tauri build`
- 型検査: `bun type`
- リント・フォーマット修正: `bun check`
- Rust 側の検証 (`src-tauri/`)
  - `cargo fmt --all`
  - `cargo clippy`
  - または `cargo lint` / `cargo format`
- 全体チェック: `mise run check` (Rust + TypeScript)

## Project Structure

```
/
├─ src/                 # フロントエンドコード
│    ├─ main.tsx       # フロントエンドエントリ
│    └─ App.tsx        # フロントエンドのルートコンポーネント
└─ src-tauri/           # Rust コードと Tauri 設定
   ├─ src/              # Rust エントリ
   │    └─ main.rs     # Rust エントリのソースコード
   └─ tauri.conf.json   # Tauri 設定ファイル
```

## Code Style

### TypeScript

- any や型アサーションは使用しない
- 自動フォーマットが可能な場合は自動フォーマットを行うこと

### Rust

- unwrap などの操作は避けて、明示的なハンドリングを行う

## Boundaries

### Always do

- 変更後に check を通す
- 実装や調査などはサブエージェントに委任する

### Ask first

- 依存パッケージの追加、メジャーバージョン更新
- tauri.config.json 、 tsconfig.json 、 biome.json などの設定ファイルの更新

### Never do

- node_modules 等の生成物を編集
- リントエラー等の ignore
