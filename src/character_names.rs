//! 전체 이름표 바탕 후보를 사용하는 비배포 삽입.
pub fn transform(source: &[u8]) -> anyhow::Result<(Vec<u8>, serde_json::Value)> {
    crate::card_surfaces::transform(source, true)
}
