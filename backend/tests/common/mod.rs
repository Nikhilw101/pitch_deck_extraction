pub fn multipart_body(
    boundary: &str,
    filename: &str,
    content_type: &str,
    payload: &[u8],
) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(b"--");
    body.extend_from_slice(boundary.as_bytes());
    body.extend_from_slice(b"\r\nContent-Disposition: form-data; name=\"file\"; filename=\"");
    body.extend_from_slice(filename.as_bytes());
    body.extend_from_slice(b"\"\r\nContent-Type: ");
    body.extend_from_slice(content_type.as_bytes());
    body.extend_from_slice(b"\r\n\r\n");
    body.extend_from_slice(payload);
    body.extend_from_slice(b"\r\n--");
    body.extend_from_slice(boundary.as_bytes());
    body.extend_from_slice(b"--\r\n");
    body
}

#[allow(dead_code)]
pub const APPLICATION_JSON: &str = "application/json";
