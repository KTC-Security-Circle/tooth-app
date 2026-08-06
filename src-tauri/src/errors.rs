use serde::ser::Serializer;

/// アプリ全体のエラー型。各バリアントは技術的詳細 (英語) を保持し、
/// `log::error!` でログ出力される。`serde::Serialize` では技術的詳細を隠し、
/// ユーザー向けの汎用メッセージ (日本語) に置き換えてフロントエンドに渡す。
/// `Validation` のみ、ユーザー向けメッセージをそのまま渡す。
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Io(String),
    #[error("{0}")]
    Config(String),
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    #[allow(dead_code)]
    Camera(String),
    #[error("{0}")]
    CoreTools(String),
    #[error("{0}")]
    Matching(String),
    #[error("{0}")]
    Internal(String),
}

/// シリアライズ用の内部表現。`AppError` の各バリアントを `{ kind, message }` 形式に
/// マッピングする。`rename_all = "camelCase"` によりバリアント名は
/// `io` / `config` / `validation` / `camera` / `coreTools` / `matching` /
/// `internal` になる。
#[derive(serde::Serialize)]
#[serde(tag = "kind", content = "message")]
#[serde(rename_all = "camelCase")]
enum AppErrorKind {
    Io(String),
    Config(String),
    Validation(String),
    Camera(String),
    CoreTools(String),
    Matching(String),
    Internal(String),
}

impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let message = match self {
            // Validation はユーザー向けメッセージをそのまま渡す
            Self::Validation(msg) => msg.clone(),
            // それ以外は技術的詳細を隠し、ユーザー向けの汎用メッセージに置き換える
            Self::Io(_) | Self::Config(_) | Self::Internal(_) => {
                "しばらくしてからもう一度お試しください".to_string()
            }
            Self::Camera(_) => "カメラを確認してください".to_string(),
            Self::CoreTools(_) => {
                "処理エンジンの起動に失敗しました。しばらくしてからもう一度お試しください"
                    .to_string()
            }
            Self::Matching(_) => {
                "マッチング処理に失敗しました。しばらくしてからもう一度お試しください".to_string()
            }
        };
        let kind = match self {
            Self::Io(_) => AppErrorKind::Io(message),
            Self::Config(_) => AppErrorKind::Config(message),
            Self::Validation(_) => AppErrorKind::Validation(message),
            Self::Camera(_) => AppErrorKind::Camera(message),
            Self::CoreTools(_) => AppErrorKind::CoreTools(message),
            Self::Matching(_) => AppErrorKind::Matching(message),
            Self::Internal(_) => AppErrorKind::Internal(message),
        };
        kind.serialize(serializer)
    }
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        // {:#} でコンテキストチェイン全体を表示 ("context: source error")
        let msg = format!("{:#}", e);
        log::error!("{}", msg);
        AppError::Internal(msg)
    }
}
