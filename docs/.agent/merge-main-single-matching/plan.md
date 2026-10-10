# 統合実施計画

1. `main` を `feature/single-matching` にコミット前マージし、未追跡・変更済み submodule を保持する。
2. Rust の設定、core-tools 通信・停止/再起動状態、照合コマンドを統合する。照合コマンドは固定参照 PLY とアプリ側で公開した一時スキャン結果のみを使う。
3. 設定フックにプロジェクターモニター選択を残し、ホーム画面のマッチング開始位置と `/scan` ルートを両立する。
4. プロトコル相関、失敗・タイムアウト、PLY 検証、同一ファイル防止、品質評価をテストし、Rust とフロントエンドの指定検証を実行する。
5. 変更をコミットせず、統合差分と検証結果を報告してレビュー承認を待つ。

## 検証記録（2026-10-10）

- `mise run type`、`mise run build` 成功。
- 変更された統合用 TypeScript/TSX 14 ファイルを `mise x -- bunx biome check` で検査。修正なし。
- 既存未追跡 `src/lib/matching.test.js` は現行 `matching.ts` / `coreTools.ts` と互換だったため `mise x -- bun test src/lib/matching.test.js` を実行し、2 件成功。
- Rust は `mise x -- cargo fmt --all -- --check`、`mise x -- cargo clippy -- -D warnings`、`mise x -- cargo test` 成功（56 tests、2026-10-10 再検証）。
- カメラ実機スキャンは実行していない。マージは未コミットで継続中。
