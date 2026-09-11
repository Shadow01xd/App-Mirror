use crate::model::{Transfer, TransferDirection, TransferStatus};
pub fn transfers() -> Vec<Transfer> {
    [
        ("foto.jpg", 3_200_000),
        ("video.mp4", 48_600_000),
        ("documento.pdf", 1_400_000),
    ]
    .iter()
    .enumerate()
    .map(|(i, (name, bytes))| Transfer {
        id: format!("demo-file-{}", i),
        file_name: name.to_string(),
        direction: TransferDirection::Send,
        total_bytes: *bytes,
        transferred_bytes: 0,
        status: TransferStatus::Queued,
    })
    .collect()
}
