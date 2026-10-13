# Stereo Scanのキャリブレーション解像度

## 調査結果

- `src-tauri/src/commands/run_matching.rs` は設定の `stereo_calibration_file` をJSONLの
  `calibration_file` に渡す。空の場合は `data/calib/stereo.yml`。バックエンドはこのパスを
  自身の作業ディレクトリから読み込むため、別の場所の同名ファイルとの混同を避けるには絶対パスを指定する。
- Mono Calibrationは入力画像の実サイズを使い、Stereo Calibrationは左右のMono YAMLと
  入力画像のサイズを照合して `image_width` / `image_height` を保存する。
- カメラの既定値は1280×720。従来のStereo ScanはStereo YAMLのサイズを検証するだけで
  カメラ設定に反映せず、既存カメラもそのまま再利用していた。
  `Camera::open` はドライバーに幅・高さを要求するが、要求どおり撮影できる保証はない。
- 撮影画像のサイズはデコード結果とmetadataに引き継がれ、復元時にStereo YAMLと比較される。
  そのため、要求サイズに応じないカメラや異なる解像度の既存カメラでは、撮影を終えた後に
  `image_size_mismatch` になる。
- 調査時点のローカル `data/calib/stereo.yml` と `tools/core/data/calib/stereo.yml` は1280×720で、
  `src-tauri/data/scans` の保存済みデコードmetadataも1280×720だった。
  報告された実行での参照ファイル・撮影サイズの組合せは記録から特定できず、実機確認が必要。

## 修正と運用

SubmoduleのStereo Scanは新規カメラをStereo YAMLの解像度で開き、左右の新しい実フレームを
撮影開始前に照合する。不一致はstage `camera`、code `image_size_mismatch` とし、実サイズ・
必要サイズ・camera role/device・参照ファイル・対応方法を返す。新しいフレームが3秒以内に
来ない場合は `camera_frame_timeout`。既存のカメラとstreamは変更しない。

不一致の場合は対象streamを停止してcameraを閉じ、再試行する。
カメラが必要な解像度に対応しない場合は、対応解像度でMono/Stereo Calibrationをやり直す。
YAMLのサイズだけの変更や撮影画像のリサイズでは対応しない。復元時の検証も維持する。
JSONLの入力項目は変更しない。

## 検証

自動テストでは解像度設定の転送、従来のcamera openの既定値、左右の不一致・フレームtimeout、
撮影前の停止、既存resourceの保持、新規resourceの解放、一致時の成功・連続scanを検証する。
既存の復元テストで `image_size_mismatch` の検出も検証する。

実機でのドライバー設定反映、不一致メッセージの実サイズ、stream停止・camera再open後の復旧、
38枚の撮影からデコード・3D復元までの成功は未検証。

今回の検証は作業ツリーと、SubmoduleのHEADに今回の変更だけを適用した `/tmp` のコピーで行った。
独立コピーでは既存CMakeのOpenEXR imported target不足を、外部の一時CMakeスクリプト
(`CMAKE_PROJECT_INCLUDE` で `find_package(OpenEXR CONFIG REQUIRED)`) で補完した。
リポジトリの設定ファイルと既存の未コミット変更はコミット対象に含めていない。
変更箇所にclang-formatを適用して再照合し、`git diff --check` も実施した。
