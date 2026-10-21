//! 錯誤型別。

/// 所有 crate 內錯誤。刻意保留來源描述，方便對照 OOXML 規格排查。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O 失敗：{0}")]
    Io(#[from] std::io::Error),

    #[error("zip 容器損毀：{0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("XML 解析失敗：{0}")]
    Xml(#[from] quick_xml::Error),

    #[error("XML 轉義失敗：{0}")]
    Escape(#[from] quick_xml::escape::EscapeError),

    #[error("找不到 part：{0}")]
    PartNotFound(String),

    #[error("找不到必要節點：{0}")]
    NodeNotFound(String),

    #[error("結構不符預期：{0}")]
    UnexpectedStructure(String),

    #[error("數值超出範圍：{0}")]
    OutOfRange(String),
}

pub type Result<T> = std::result::Result<T, Error>;
