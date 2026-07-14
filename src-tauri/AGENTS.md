# AGENTS.md (src-tauri)

Tauri (Rust) 側のコードを扱う際の指針。プロジェクト全体の設定はルートの [`AGENTS.md`](../AGENTS.md) を参照。

## Project Structure

```
src-tauri/
├─ Cargo.toml              # Rust 依存関係
├─ tauri.conf.json         # Tauri 設定ファイル
├─ capabilities/
│    └─ default.json       # Tauri 権限 (permissions) 設定
└─ src/
   ├─ main.rs              # Tauri エントリ (tooth_app_lib::run を呼ぶのみ)
   ├─ lib.rs               # Tauri ビルダー設定・コマンド登録
   ├─ errors.rs            # アプリ全体のエラー型 (AppError)
   ├─ commands/            # Tauri コマンドの実装
   ├─ state/               # データモデル・状態構造体
   └─ utils/               # ユーティリティ関数
```

## Module Conventions

### `mod.rs` はエクスポートのみ

`mod.rs` にはモジュール宣言 (`pub mod ...`) と必要最小限の再エクスポート (`pub use ...`) のみ記述する。実装・構造体定義・関数は `mod.rs` に書かず、専用のファイルに切り出す。

### `commands/` — 1ファイル1コマンド

Tauri コマンドは1ファイルに1つのコマンド関数を配置する。そのコマンド固有のヘルパー関数があれば同じファイルに書いてよい。複数のコマンドを1ファイルにまとめない。

コマンドを追加する手順:

1. `commands/<command_name>.rs` を作成し `#[tauri::command]` 関数を定義
2. `commands/mod.rs` に `pub mod <command_name>;` を追加
3. `lib.rs` の `generate_handler!` に `commands::<command_name>::<command_name>` を追加

### `state/` — データモデル

Tauri の状態管理やコマンド間で共有されるデータ構造 (構造体・enum) を配置する。`Settings` のようにフロントエンドと JSON でやり取りする構造体は `#[serde(rename_all = "camelCase")]` を付ける。データモデルが JSON ファイルフォーマットを所有する場合、モデル自身に `load` / `save` メソッドを持たせ、コマンドはモデルに委譲する薄いラッパーとする。

### `utils/` — ユーティリティ関数

コマンド横断で使う純粋なヘルパー関数を配置する。Tauri の `AppHandle` を受け取るパス系ユーティリティなど。

### `errors.rs` — エラー型

アプリ全体のエラー型 `AppError` を定義する。`thiserror::Error` でエラー種別を enum で表現し、`serde::Serialize` を手動実装してフロントエンドに `{ "kind": "...", "message": "..." }` の構造化エラーとして渡す。内部ヘルパー関数 (`utils/`) では `anyhow::Result` と `.context()` を使い、コマンド境界で `AppError` に変換する (`From<anyhow::Error> for AppError` により `?` で自動変換される)。

## Code Style

- `unwrap` / `expect` / `panic!` は避け、明示的なエラーハンドリング (`Result` + `AppError`) を行う
- コマンドの戻り値は `Result<T, AppError>` とし、エラーメッセージにコンテキスト (パス・原因) を含める
- エラー型は `src/errors.rs` の `AppError` (thiserror + serde::Serialize) を使う。内部ヘルパーでは `anyhow::Result` + `.context()` でコンテキストを付与し、コマンド境界で `AppError` に変換する
- ファイルが存在しないなどの「期待される不在」はエラーにせずデフォルト値を返すなど、呼び出し側が困らない挙動にする
- CI の clippy は `cargo clippy -- -D warnings` で警告をエラー扱いする

## Logging

`tauri-plugin-log` + `log` クレートでロギングを統一する。`println!` / `eprintln!` は使わず、`log` マクロ (`log::error!` / `log::warn!` / `log::info!` / `log::debug!`) を使う。

### ログレベルの目安

| レベル | 用途 |
|--------|------|
| `error!` | インフラエラー・IOエラー・予期しない失敗 |
| `warn!` | リカバリ可能な問題 (カメラを開けない、設定パース失敗でデフォルトフォールバックなど) |
| `info!` | 通常の操作ログ |
| `debug!` | 開発時の詳細トレース |

### 設定 (lib.rs)

- 出力先: Stdout (常時) + Webview (開発時のみ) + LogDir (本番のみ)
- ログレベル: 開発時 `Debug`、本番 `Info` (`cfg!(debug_assertions)` で切り替え)
- ファイルローテーション: 10MB ごと、`KeepOne` 戦略 (本番のみ)
- フロントエンド: `@tauri-apps/plugin-log` の `attachConsole()` で Rust のログをブラウザコンソールに転送
