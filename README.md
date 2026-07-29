# tooth-app

Tauri + React + TypeScript によるデスクトップアプリケーション。
画像処理バックエンド (`tools/core/` の C++ サイドカー `tooth-backend`) を同梱する。

## 必要環境

- [mise](https://mise.jdx.dev/) (ツールチェイン管理)
- [Bun](https://bun.sh/) (mise 経由で自動インストール)
- Rust (mise 経由で自動インストール)
- Docker は不要 (ホストビルド方式)

### システム依存

OpenCV の静的ビルドに以下のシステムパッケージが必要:

- `pkg-config`
- `cmake`, `ninja`
- `libgtk-3-dev` (HighGUI の GTK3 バックエンド)
- `liblapack-dev`, `libblas-dev`, `libcblas-dev`, `liblapacke-dev`
- `git` (OpenCV ソースの clone)

`mise run setup` が OpenCV 4.10 の静的ビルドを `~/.local/opencv/4.10.0-static` に自動構築する。

## セットアップ

```sh
mise run setup
```

以下を順に実行:
1. OpenCV 4.10 静的ビルド (`tools/core/scripts/build_opencv_4_10_static.sh`)
2. Git submodule 初期化
3. `bun install`

## 開発

```sh
# デスクトップアプリを起動 (サイドカー含む)
mise run dev

# フロントエンドのみ起動 (port 1420)
mise run dev:web
```

## ビルド

```sh
# サイドカー (tooth-backend) のみビルド
mise run build:core-tools

# サイドカー + デスクトップアプリをビルド
mise run build:all
```

ビルド成果物は `src-tauri/binaries/tooth-backend-<host-tuple>` に配置される。

## 検証

```sh
# 全体チェック (Rust + TypeScript + core-tools)
mise run check
```

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)